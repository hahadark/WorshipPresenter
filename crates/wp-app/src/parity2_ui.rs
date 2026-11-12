//! [트랙 A] 기능 대조 2차: 남은 '일부' 채우기
//!  - 성경 책 목록 창 (Ctrl+B, Electron control.js openBooks): 책 → 장 → 미리보기
//!  - 종류 골라 붙여 넣기 창 (Ctrl+Shift+V, Electron paste.js pasteAs): 성경 · 찬양 · 공지 · 제목 · 슬라이드 · 그림
//!  - 미리보기 · LIVE 썸네일 오른쪽 클릭 (Electron ctxmenu.js slideMenu)

use crate::app::{Act, App};
use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::presentation::Deck;

#[derive(Default)]
struct St {
    books: bool,
    book: Option<i64>,
    paste: bool,
    paste_text: String,
    paste_as: String,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

/// Ctrl+B · Ctrl+Shift+V (설정 단축키 openBooks 를 따른다)
pub fn shortcuts(app: &mut App, ctx: &egui::Context) {
    let st = json!({ "shortcuts": app.core.lock().settings.get()["shortcuts"].clone() });
    if crate::keys::pressed(ctx, &st, "openBooks", "Ctrl+B") {
        ST.lock().books = true; // 닫기는 창의 X (한 프레임을 두 번 그려도 같은 결과)
    }
    // Ctrl+Shift+V: egui 는 Ctrl+V 를 Key 가 아니라 Paste 이벤트로 준다 → Shift 가 눌린 Paste 면 이 창으로
    let pasted = ctx.input(|i| if i.modifiers.shift { i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }) } else { None });
    let focused = ctx.egui_wants_keyboard_input(); // 글 입력칸에 있을 때만 (단추 초점은 상관없음)
    if let Some(t) = pasted.filter(|_| !focused) {
        let mut s = ST.lock();
        s.paste_text = t.trim_end().to_string();
        if s.paste_as.is_empty() {
            s.paste_as = "auto".into();
        }
        s.paste = true;
    } else if !focused && ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::V))) {
        open_paste(); // 클립보드에 글이 없을 때 (슬라이드·그림)
    }
}

/// 클립보드 글을 읽어 '종류 골라 붙여 넣기' 창을 연다
pub fn open_paste() {
    let t = wp_core::clippaste::run_ps("[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-Clipboard -Raw", &[]).unwrap_or_default();
    let mut s = ST.lock();
    s.paste_text = t.trim_end().to_string();
    if s.paste_as.is_empty() {
        s.paste_as = "auto".into();
    }
    s.paste = true;
}

/// 점검용: --open-a2 books|paste
fn open_from_args() {
    let a: Vec<String> = std::env::args().collect();
    if let Some(v) = a.iter().position(|x| x == "--open-a2").and_then(|i| a.get(i + 1)) {
        for id in v.split(',') {
            match id {
                "books" => ST.lock().books = true,
                "paste" => open_paste(),
                _ => {}
            }
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(open_from_args);
    books_win(app, ctx);
    paste_win(app, ctx, acts);
}

// ---------------- 성경 책 목록 ----------------
fn books_win(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.books {
        return;
    }
    let mut open = true;
    let (structure, vname) = {
        let mut c = app.core.lock();
        let v = c.version_id();
        let name = c.store.list_versions().into_iter().find(|x| x.id == v).map(|x| x.name).unwrap_or_default();
        (c.store.get_structure(&v), name)
    };
    let mut pick: Option<(i64, i64)> = None;
    egui::Window::new(wp_core::i18n::tr_args("성경 책 ({1}) — Ctrl+B", &[&vname])).open(&mut open).default_size([640.0, 520.0]).show(ctx, |ui| {
        ui.columns(2, |cols| {
            for (ci, (test, label)) in [("OT", "구약"), ("NT", "신약")].iter().enumerate() {
                let ui = &mut cols[ci];
                ui.label(RichText::new(wp_core::i18n::tr(label)).strong());
                egui::ScrollArea::vertical().id_salt(format!("books-{test}")).max_height(220.0).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for b in structure.iter().filter(|b| b["testament"].as_str().unwrap_or("") == *test) {
                            let id = b["id"].as_i64().unwrap_or(0);
                            if ui.selectable_label(st.book == Some(id), b["name"].as_str().unwrap_or("")).clicked() {
                                st.book = Some(id);
                            }
                        }
                    });
                });
            }
        });
        ui.separator();
        if let Some(b) = st.book.and_then(|id| structure.iter().find(|x| x["id"] == id)) {
            ui.label(RichText::new(wp_core::i18n::tr_args("{1} — 장을 누르면 미리보기", &[&b["name"].as_str().unwrap_or("")])).strong().color(Color32::from_rgb(0xff, 0xd4, 0x79)));
            egui::ScrollArea::vertical().id_salt("chapters").max_height(180.0).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (k, n) in b["chapters"].as_array().into_iter().flatten().enumerate() {
                        if n.as_i64().unwrap_or(0) > 0 && ui.add(egui::Button::new((k + 1).to_string()).min_size(egui::vec2(34.0, 24.0))).on_hover_text(wp_core::i18n::tr_args("{1}절", &[&n])).clicked() {
                            pick = Some((b["id"].as_i64().unwrap_or(0), k as i64 + 1));
                        }
                    }
                });
            });
        } else {
            ui.label(RichText::new(wp_core::i18n::tr("책을 고르세요.")).color(Color32::GRAY));
        }
    });
    if let Some((book, ch)) = pick {
        let mut c = app.core.lock();
        let v = c.version_id();
        let r = c.store.get_chapter(&v, book, ch);
        if r["ok"] == json!(true) {
            let p = r["passage"].clone();
            let q = p["title"].as_str().unwrap_or("").to_string();
            c.preview_passage(&p, &q, None);
            drop(c);
            app.search = q.clone();
            app.result = json!({ "ok": true, "mode": "reference", "passage": p });
        } else {
            c.notify(r["error"].as_str().unwrap_or(&wp_core::i18n::tr("본문을 찾지 못했습니다.")), "warn");
        }
    }
    if !open {
        st.books = false;
    }
}

// ---------------- 종류 골라 붙여 넣기 ----------------
fn paste_win(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let mut st = ST.lock();
    if !st.paste {
        return;
    }
    let mut open = true;
    let mut close = false;
    egui::Window::new(wp_core::i18n::tr("종류를 골라 붙여 넣기 (Ctrl+Shift+V)")).open(&mut open).default_size([560.0, 420.0]).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(wp_core::i18n::tr("넣을 종류"));
            for (v, l) in [("auto", "알아서"), ("bible", "성경 (줄마다 장절)"), ("song", "찬양 가사"), ("text", "공지"), ("title", "제목")] {
                ui.radio_value(&mut st.paste_as, v.to_string(), wp_core::i18n::tr(l));
            }
        });
        egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut st.paste_text).desired_rows(10).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("클립보드에 글이 없습니다. 여기에 붙여 넣거나 쓰세요.")));
        });
        ui.horizontal(|ui| {
            if ui.button(wp_core::i18n::tr("다시 읽기")).on_hover_text(wp_core::i18n::tr("클립보드 글을 다시 가져옵니다")).clicked() {
                st.paste_text = wp_core::clippaste::run_ps("[Console]::OutputEncoding = [Text.Encoding]::UTF8; Get-Clipboard -Raw", &[]).unwrap_or_default().trim_end().to_string();
            }
            if ui.button(RichText::new(wp_core::i18n::tr("글로 넣기")).strong()).clicked() {
                let as_ = if st.paste_as == "auto" { "" } else { st.paste_as.as_str() };
                let plan = {
                    let mut c = app.core.lock();
                    let v = c.version_id();
                    let cell = std::cell::RefCell::new(&mut c.store);
                    let is_ref = |l: &str| cell.borrow_mut().resolve_reference(&v, l)["ok"] == json!(true);
                    if as_ == "title" { Some(("title".to_string(), title_items(&st.paste_text))) } else { wp_core::clippaste::plan_text(&st.paste_text, as_, &is_ref) }
                };
                match plan {
                    Some((kind, items)) if !items.is_empty() => {
                        let n = items.len();
                        for x in items {
                            let ty: &'static str = match x["type"].as_str() {
                                Some("bible") => "bible",
                                Some("song") => "song",
                                Some("title") => "title",
                                _ => "text",
                            };
                            acts.push(Act::AddItem(ty, Some(x["extra"].clone())));
                        }
                        app.core.lock().notify(wp_core::i18n::tr_args("{1} 순서 {2}개를 넣었습니다.", &[&wp_core::i18n::tr(kind_name(&kind)), &n]), "ok");
                        close = true;
                    }
                    _ => app.core.lock().notify(wp_core::i18n::tr("넣을 글이 없습니다."), "warn"),
                }
            }
            ui.separator();
            if ui.button(wp_core::i18n::tr("PowerPoint 슬라이드로")).on_hover_text(wp_core::i18n::tr("PowerPoint 에서 복사한 슬라이드 → 찬송가·PPT 순서")).clicked() {
                grab(app, true);
                close = true;
            }
            if ui.button(wp_core::i18n::tr("그림으로")).clicked() {
                grab(app, false);
                close = true;
            }
        });
    });
    if !open || close {
        st.paste = false;
    }
}

fn kind_name(k: &str) -> &'static str {
    match k {
        "bible" => "성경",
        "song" => "찬양",
        "title" => "제목",
        _ => "공지",
    }
}

/// 제목: 줄마다 한 장 (첫 줄 = 제목, 둘째 줄부터 '---' 장)
fn title_items(text: &str) -> Vec<Value> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let Some(first) = lines.first() else { return vec![] };
    vec![json!({ "type": "title", "extra": { "title": first, "more": lines[1..].join("\n---\n") } })]
}

/// 슬라이드(강제) · 그림 → 새 순서 (백그라운드)
fn grab(app: &mut App, slides: bool) {
    let dd = app.core.lock().data_dir.clone();
    app.core.lock().notify(wp_core::i18n::tr("클립보드를 읽는 중…"), "info");
    wp_core::hymns::run_bg(
        move || {
            if slides {
                wp_core::clippaste::grab_slides(&dd, true).map(|(f, _)| (true, f.to_string_lossy().to_string()))
            } else {
                wp_core::clippaste::grab_image(&dd).map(|u| (false, u))
            }
        },
        |c, r| match r {
            Ok((true, f)) => {
                let h = wp_core::hymns::HymnFile { number: None, title: "붙여 넣은 슬라이드".into(), name: String::new(), folder: String::new(), file: std::path::PathBuf::from(&f) };
                c.add_item(wp_core::hymns::new_hymn_item(&h), None);
                c.notify(wp_core::i18n::tr("PowerPoint 슬라이드를 새 순서로 넣었습니다."), "ok");
            }
            Ok((false, url)) => {
                let mut it = wp_core::core::new_item("image", "붙여 넣은 그림");
                it["image"] = json!(url);
                c.add_item(it, None);
                c.notify(wp_core::i18n::tr("그림을 새 순서로 넣었습니다."), "ok");
            }
            Err(e) => c.notify(e, "warn"),
        },
    );
}

// ---------------- 썸네일 오른쪽 클릭 ----------------
/// 미리보기·LIVE 슬라이드 단추 하나의 오른쪽 클릭 메뉴
pub fn slide_menu(core: &Shared, r: &egui::Response, d: &Deck, k: usize, is_live: bool, acts: &mut Vec<Act>) {
    r.context_menu(|ui| {
        ui.set_min_width(200.0);
        if is_live {
            if ui.button("여기로 (이 슬라이드 송출)").clicked() {
                acts.push(Act::LiveGoto(k));
                ui.close();
            }
            if ui.button("글자 숨김·복원").clicked() {
                acts.push(Act::ToggleClear);
                ui.close();
            }
            return;
        }
        if ui.button("▶ 바로 송출").clicked() {
            acts.push(Act::PreviewIndex(k));
            acts.push(Act::SendPreview);
            ui.close();
        }
        let ci = d.passage["cueIndex"].as_i64().filter(|i| *i >= 0).map(|i| i as usize);
        let Some(i) = ci else { return };
        let it = core.lock().cue.as_ref().and_then(|c| c.items.get(i)).cloned();
        let Some(it) = it else { return };
        let slide = &d.slides[k];
        // 글 덩어리 번호(찬양·공지) 또는 슬라이드 번호
        // 건너뛴 슬라이드는 미리보기에 없으니 원래 번호로 되돌린다
        let mut orig = k;
        let mut sk: Vec<usize> = it["skip"].as_array().into_iter().flatten().filter_map(|x| x.as_u64().map(|v| v as usize)).collect();
        sk.sort();
        for s in sk {
            if s <= orig {
                orig += 1;
            }
        }
        let key = slide["src"].as_u64().map(|x| x as usize).unwrap_or(orig);
        let ty = it["type"].as_str().unwrap_or("");
        let chunk = ty == "song" || ty == "text";
        ui.separator();
        let mut op = None;
        if chunk && ui.button("＋ 이 다음에 새 슬라이드").clicked() {
            op = Some("add");
        }
        if chunk && ui.button("⧉ 이 슬라이드 복제").clicked() {
            op = Some("dup");
        }
        if chunk && key > 0 && ui.button("여기서 새 순서로 나누기 (구역)").clicked() {
            op = Some("split");
        }
        if !chunk && ty != "bible" && ui.button("이 슬라이드 건너뛰기").clicked() {
            op = Some("skip");
        }
        if let Some(op) = op {
            let (new_it, back) = wp_core::slideedit::apply_op(&it, op, key);
            let mut c = core.lock();
            c.remember_items();
            if let Some(cue) = &mut c.cue {
                if i < cue.items.len() {
                    cue.items[i] = new_it;
                    if let Some(b) = back {
                        cue.items.insert(i + 1, b);
                    }
                }
            }
            c.save_cue();
            let idx = if op == "add" || op == "dup" { k + 1 } else { k.min(d.slides.len().saturating_sub(1)) };
            if let Ok(mut deck) = c.item_deck(i) {
                deck.index = idx.min(deck.slides.len().saturating_sub(1));
                c.pres.preview = Some(deck);
            }
            c.emit(wp_core::core::Ev::Ui);
            ui.close();
        }
    });
}
