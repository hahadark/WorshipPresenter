//! 여덟 패널의 탭·분할·독립 창 도킹. 코어 잠금은 설정 복사/반영 때만 잡는다.
use crate::app::{Act, App, Snap};
use eframe::egui::{self, Rect, Sense, UiBuilder};
use serde_json::Value;
use wp_core::i18n::tr;
use wp_core::workspace::{Node, Pane, Workspace};
pub struct WorkspaceUi {
    pub open: bool,
    pub state: Workspace,
    #[cfg(test)]
    pub(crate) observed: std::collections::BTreeMap<String, Rect>,
    name: String,
    persisted: Value,
    pending: Option<(Pane, Pane, u8)>,
    float: Option<Pane>,
    float_group: Option<Pane>,
    merge_group: Option<(Pane, Pane)>,
    targets: Vec<Pane>,
}
impl WorkspaceUi {
    pub fn new(s: &Value) -> Self {
        let state = Workspace::load(s);
        let persisted = serde_json::to_value(&state).unwrap();
        Self {
            open: false,
            #[cfg(test)]
            observed: Default::default(),
            state,
            name: String::new(),
            persisted,
            pending: None,
            float: None,
            float_group: None,
            merge_group: None,
            targets: Vec::new(),
        }
    }
}
impl App {
    /// 작업공간 · 패널 배치 창 (고르기는 머리 줄 topbar_ui::workspace_picker, 원본 .ws-pick)
    pub(crate) fn workspace_window(&mut self, ctx: &egui::Context) {
        let mut open = self.workspace.open;
        crate::widgets::close_on_outside(ctx, egui::Id::unique("wp-float-workspace"), &mut open);
        egui::Window::new(tr("작업공간 · 패널 배치")).id(egui::Id::unique("wp-float-workspace")).default_pos(crate::widgets::float_pos(ctx, egui::Id::unique("wp-float-workspace"), 420.0)).open(&mut open).show(ctx,|ui|{
   ui.label(tr("제목을 끌어 다른 패널의 좌·우·위·아래에 놓습니다. 경계선을 끌면 크기가 바뀝니다."));
   ui.add_enabled_ui(!self.workspace.state.locked,|ui|{ui.horizontal_wrapped(|ui|{for p in Pane::ALL{let mut visible=self.workspace.state.visible(p);if ui.checkbox(&mut visible,tr(p.title())).changed(){self.workspace.state.toggle(p)}}});
   if ui.button(tr("기본 배치로 복원")).clicked(){self.workspace.state.select("b-default")}});
   ui.separator();ui.horizontal(|ui|{ui.text_edit_singleline(&mut self.workspace.name);if ui.button(tr("지금 배치로 새로 저장")).clicked(){self.workspace.state.save_as(&self.workspace.name);self.workspace.name.clear();}});
   let mut use_id=None;let mut delete=None;let mut up=None;let layout=self.workspace.state.layout.clone();let floating=self.workspace.state.floating.clone();
   for s in &mut self.workspace.state.saved{ui.horizontal(|ui|{ui.text_edit_singleline(&mut s.name);if s.name.trim().is_empty(){s.name=tr("작업공간")}s.name=s.name.chars().take(40).collect();if ui.button(tr("선택")).clicked(){use_id=Some(s.id.clone())}if ui.add_enabled(!s.name.trim().is_empty(),egui::Button::new(tr("배치 덮어쓰기"))).clicked(){s.layout=layout.clone();s.floating=floating.clone()}if ui.button(tr("위로")).clicked(){up=Some(s.id.clone())}if ui.button(tr("삭제")).clicked(){delete=Some(s.id.clone())}});}
   if let Some(id)=up{self.workspace.state.move_up(&id)}
   if let Some(id)=use_id{self.workspace.state.select(&id)}if let Some(id)=delete{self.workspace.state.delete(&id)}
   ui.separator();ui.label(tr("Electron 배치는 원본을 보존하며 Rust 배치는 별도 저장합니다. 가운데 놓으면 탭 합치기, 가장자리에 놓으면 분할합니다. 탭 메뉴에서 띄우기·닫기가 가능합니다."));
  });
        self.workspace.open = open;
    }
    pub(crate) fn workspace_content(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        #[cfg(test)]
        self.workspace.observed.clear();
        // 송출 조작은 늘 보이는 아래 송출 줄(topbar_ui::bottom_bar)에 있다.
        self.workspace.targets = Pane::ALL
            .into_iter()
            .filter(|p| self.workspace.state.visible(*p))
            .collect();
        let mut layout = self.workspace.state.layout.take();
        let rect = ui.available_rect_before_wrap();
        if let Some(n) = &mut layout {
            self.workspace_node(ui, rect, n, s, acts, 1, false);
        } else {
            ui.label(tr(
                "표시된 패널이 없습니다. 위쪽 ‘저장 · 관리 · 패널’에서 다시 표시하세요.",
            ));
        }
        self.workspace.state.layout = layout;
        if let Some(p) = ui.data_mut(|d| {
            d.remove_temp::<Option<Pane>>(egui::Id::unique("workspace-hide"))
                .flatten()
        }) {
            self.workspace.state.toggle(p)
        }
        if let Some((p, t, side)) = self.workspace.pending.take() {
            self.workspace.state.move_to(p, t, side)
        }
        if let Some(p) = self.workspace.float.take() {
            self.workspace.state.float(p)
        }
        if let Some(p) = self.workspace.float_group.take() {
            self.workspace.state.float_group(p);
        }
        if let Some((p, t)) = self.workspace.merge_group.take() {
            self.workspace.state.merge_group(p, t);
        }
        // 떠 있는 묶음: 원본(dockview addFloatingGroup)처럼 프로그램 창 안에 떠 있는 패널로 그린다.
        // (별도 OS 창으로 띄우면 다른 창 뒤로 숨고, 매 프레임 위치·크기를 다시 주면서 창이 깜빡였다)
        let screen = ui.ctx().content_rect();
        for f in self.workspace.state.floating.clone() {
            let ctx = ui.ctx().clone();
            let mut attach = false;
            let mut node = f.node();
            let size = f.size.unwrap_or([520.0, 420.0]);
            let size = egui::vec2(size[0].min(screen.width() - 40.0).max(240.0), size[1].min(screen.height() - 80.0).max(160.0));
            // 예전에 OS 창 자리로 저장된 값이 화면 밖이면 화면 안으로
            let pos = f.position.map(|p| egui::pos2(p[0], p[1])).unwrap_or(screen.center() - size / 2.0);
            let pos = egui::pos2(pos.x.clamp(screen.left(), (screen.right() - 120.0).max(screen.left())), pos.y.clamp(screen.top(), (screen.bottom() - 60.0).max(screen.top())));
            let mut open = true;
            let resp = egui::Window::new(tr(f.pane.title()))
                .id(egui::Id::new(("workspace-float", format!("{:?}", f.pane))))
                .default_pos(pos)
                .default_size(size)
                .collapsible(false)
                .resizable(true)
                .constrain(true)
                .open(&mut open)
                .show(&ctx, |ui| {
                    if ui
                        .add_enabled(!self.workspace.state.locked, egui::Button::new(tr("제자리에 붙이기")))
                        .on_hover_text(tr("이 묶음을 다시 프로그램 배치 안으로 넣습니다"))
                        .clicked()
                    {
                        attach = true
                    }
                    let rect = ui.available_rect_before_wrap();
                    self.workspace_node(ui, rect, &mut node, s, acts, 100 + f.pane as u64, false);
                    ui.allocate_rect(rect, Sense::hover());
                });
            if !open {
                attach = true
            }
            // 자리·크기는 사용자가 옮기거나 늘렸을 때만 저장 (반올림으로 매 프레임 값이 바뀌지 않게)
            let geometry = resp.map(|r| {
                let rr = r.response.rect;
                ([rr.min.x.round(), rr.min.y.round()], [rr.width().round(), rr.height().round()])
            });
            if let Some(saved) = self
                .workspace
                .state
                .floating
                .iter_mut()
                .find(|x| x.pane == f.pane)
            {
                saved.group = Some(node);
                if let Some((position, size)) = geometry {
                    if size.iter().all(|x| *x >= 100.0) {
                        let moved = |a: Option<[f32; 2]>, b: [f32; 2]| a.is_none_or(|a| (a[0] - b[0]).abs() >= 2.0 || (a[1] - b[1]).abs() >= 2.0);
                        if moved(saved.position, position) {
                            saved.position = Some(position);
                        }
                        if moved(saved.size, size) {
                            saved.size = Some(size);
                        }
                    }
                }
            }
            if attach {
                self.workspace.state.attach_group(f.pane)
            }
        }
        let value = serde_json::to_value(&self.workspace.state).unwrap();
        if value != self.workspace.persisted {
            let mut c = self.core.lock();
            c.settings.update(|s| s["rsWorkspace"] = value.clone());
            self.workspace.persisted = value;
        }
    }
    fn workspace_node(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        n: &mut Node,
        s: &Snap,
        acts: &mut Vec<Act>,
        id: u64,
        foldable: bool,
    ) {
        if let Node::Pane(p) = n {
            *n = Node::tabs(vec![*p]);
        }
        match n {
            Node::Split {
                horizontal,
                ratio,
                a,
                b,
            } => {
                let len = if *horizontal {
                    rect.width()
                } else {
                    rect.height()
                };
                let folded = |n: &Node| {
                    matches!(
                        n,
                        Node::Tabs {
                            collapsed: true,
                            ..
                        }
                    )
                };
                let at = if !*horizontal && folded(a) {
                    34.0
                } else if !*horizontal && folded(b) {
                    (len - 34.0).max(0.0)
                } else {
                    (len - 6.0).max(0.0) * *ratio
                };
                let (mut ra, mut rb, mut divider) = (rect, rect, rect);
                if *horizontal {
                    ra.max.x = rect.min.x + at;
                    divider.min.x = ra.max.x;
                    divider.max.x = ra.max.x + 6.0;
                    rb.min.x = divider.max.x;
                } else {
                    ra.max.y = rect.min.y + at;
                    divider.min.y = ra.max.y;
                    divider.max.y = ra.max.y + 6.0;
                    rb.min.y = divider.max.y;
                }
                let response = ui.interact(
                    divider,
                    ui.id().with(("split", id)),
                    if self.workspace.state.locked {
                        Sense::hover()
                    } else {
                        Sense::drag()
                    },
                );
                if response.dragged() {
                    if let Some(pos) = response.interact_pointer_pos() {
                        let offset = if *horizontal {
                            pos.x - rect.min.x
                        } else {
                            pos.y - rect.min.y
                        };
                        Node::resize_ratio(ratio, a, b, offset / (len - 6.0).max(1.0));
                    }
                }
                // 카드 사이 틈: 평소에는 빈 칸(bg), 끌 수 있을 때 올리면 accent 45% (--dv-active-sash-color)
                if response.hovered() || response.dragged() {
                    ui.painter().rect_filled(
                        divider.shrink(1.0),
                        2.0,
                        crate::theme::alpha(crate::theme::pal().accent, 0.45),
                    );
                }
                self.workspace_node(ui, ra, a, s, acts, id * 2, !*horizontal);
                self.workspace_node(ui, rb, b, s, acts, id * 2 + 1, !*horizontal);
            }
            Node::Pane(_) => unreachable!(),
            Node::Tabs {
                panes,
                active,
                collapsed,
            } => {
                // 점검용 --pane library: 그 탭이 있는 묶음에서 그 탭을 앞으로 (스크린샷 점검)
                if let Some(want) = std::env::args().position(|x| x == "--pane").and_then(|i| std::env::args().nth(i + 1)) {
                    if let Some(t) = panes.iter().copied().find(|t| format!("{t:?}").eq_ignore_ascii_case(&want)) {
                        if *active != t { *active = t; *collapsed = false; }
                    }
                }
                let p = *active;
                let locked = self.workspace.state.locked;
                // 독 패널 카드 (Electron .wc-dock .dv-groupview: panel 바탕, 둥글기 12, line-soft 테두리)
                {
                    let pl = crate::theme::pal();
                    ui.painter().rect(
                        rect,
                        egui::CornerRadius::same(crate::theme::CARD_RADIUS),
                        pl.panel,
                        egui::Stroke::new(1.0, pl.line_soft),
                        egui::StrokeKind::Inside,
                    );
                }
                ui.scope_builder(
                    UiBuilder::new().max_rect(rect).id_salt(("pane", id)),
                    |ui| {
                        ui.set_clip_rect(rect);
                        ui.set_min_size(rect.size());
                        ui.spacing_mut().item_spacing.y = 0.0;
                        // 패널 탭 머리: 제목 + ✕(활성 탭) · 흐린 비활성 탭 · accent 밑줄 · 오른쪽 ▾(메뉴) · ⧉(독립 창)
                        let titles: Vec<String> = panes.iter().map(|t| tr(t.title())).collect();
                        let refs: Vec<&str> = titles.iter().map(|t| t.as_str()).collect();
                        let mut idx = panes.iter().position(|t| *t == *active).unwrap_or(0);
                        let head = crate::widgets::panel_tabs_dock(
                            ui,
                            &format!("dock-{id}"),
                            &refs,
                            &mut idx,
                            !locked,
                            !locked,
                            !locked,
                        );
                        for (k, tab) in panes.iter().copied().enumerate() {
                            let Some(r) = head.tabs.get(k) else { continue };
                            #[cfg(test)]
                            self.workspace.observed.insert(format!("tab-{tab:?}"), r.rect);
                            #[cfg(test)]
                            self.workspace.observed.insert(format!("pane-{tab:?}"), rect);
                            if !locked {
                                r.dnd_set_drag_payload(tab);
                            }
                            r.context_menu(|ui| {
                                ui.add_enabled_ui(!locked, |ui| {
                                    if ui.button(tr("독립창으로 띄우기")).clicked() {
                                        self.workspace.float = Some(tab);
                                        ui.close()
                                    }
                                    if ui.button(tr("패널 숨기기")).clicked() {
                                        ui.data_mut(|d| {
                                            d.insert_temp(
                                                egui::Id::unique("workspace-hide"),
                                                Some(tab),
                                            )
                                        });
                                        ui.close()
                                    }
                                    // 예전 ▾ 펼침에 있던 묶음 동작은 여기(탭 오른쪽 클릭)로
                                    ui.separator();
                                    if ui.button(tr("그룹 전체 독립창")).clicked() {
                                        self.workspace.float_group = Some(p);
                                        ui.close();
                                    }
                                    ui.menu_button(tr("그룹 전체 병합"), |ui| {
                                        for target in self.workspace.targets.clone() {
                                            if !panes.contains(&target) && ui.button(tr(target.title())).clicked() {
                                                self.workspace.merge_group = Some((p, target));
                                                ui.close();
                                            }
                                        }
                                    });
                                });
                            });
                        }
                        if let Some(k) = head.clicked {
                            *active = panes[k];
                            *collapsed = false
                        }
                        if let Some(k) = head.closed {
                            // 탭 ✕ = 그 패널 숨기기 (Electron: 패널 닫기 → 작업공간 패널 목록에서 다시 표시)
                            let tab = panes[k];
                            ui.data_mut(|d| {
                                d.insert_temp(egui::Id::unique("workspace-hide"), Some(tab))
                            });
                        }
                        if head.popout {
                            self.workspace.float_group = Some(p);
                        }
                        // ▾ = 접기·펼치기 바로 (원본 dock.js headerActions: 누르면 toggleCollapse, 다른 메뉴 없음)
                        if let Some(fr) = &head.fold_resp {
                            let fr = fr.clone().on_hover_text(tr(if *collapsed {
                                "펼치기"
                            } else if foldable {
                                "접기"
                            } else {
                                "위·아래로 나뉜 패널만 접을 수 있습니다."
                            }));
                            if fr.clicked() {
                                if foldable || *collapsed {
                                    *collapsed = !*collapsed;
                                } else {
                                    self.core.lock().notify(tr("위·아래로 나뉜 패널만 접을 수 있습니다."), "info");
                                }
                            }
                        }
                        if !self.workspace.state.locked {
                            if let Some(payload) = egui::DragAndDrop::payload::<Pane>(ui.ctx()) {
                                if *payload != p {
                                    if let Some(pos) = ui
                                        .ctx()
                                        .pointer_hover_pos()
                                        .filter(|pos| rect.contains(*pos))
                                    {
                                        let x = (pos.x - rect.min.x) / rect.width().max(1.0);
                                        let y = (pos.y - rect.min.y) / rect.height().max(1.0);
                                        let side = if x > 0.25 && x < 0.75 && y > 0.25 && y < 0.75 {
                                            4
                                        } else if x.min(1.0 - x) < y.min(1.0 - y) {
                                            if x < 0.5 { 0 } else { 1 }
                                        } else if y < 0.5 {
                                            2
                                        } else {
                                            3
                                        };
                                        let mut highlight = rect;
                                        match side {
                                            0 => highlight.max.x = rect.center().x,
                                            1 => highlight.min.x = rect.center().x,
                                            2 => highlight.max.y = rect.center().y,
                                            _ => highlight.min.y = rect.center().y,
                                        };
                                        ui.painter().rect_filled(
                                            highlight,
                                            0.0,
                                            crate::theme::alpha(crate::theme::pal().accent, 0.22),
                                        );
                                        if ui.input(|i| i.pointer.any_released()) {
                                            if let Some(payload) =
                                                egui::DragAndDrop::take_payload::<Pane>(ui.ctx())
                                            {
                                                self.workspace.pending = Some((*payload, p, side));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !*collapsed {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            egui::ScrollArea::vertical()
                                .id_salt(("body", id))
                                .auto_shrink([false, false])
                                .show(ui, |ui| self.workspace_panel(ui, *active, s, acts));
                        }
                    },
                );
            }
        }
    }
    fn workspace_panel(&mut self, ui: &mut egui::Ui, p: Pane, s: &Snap, acts: &mut Vec<Act>) {
        match p {
            Pane::Cue => self.workspace_cue(ui, s, acts),
            Pane::Main => self.main_panel(ui, s, acts),
            Pane::Quick => self.quick_panel(ui, s, acts),
            Pane::Library => self.library_tabs_panel(ui, acts),
            Pane::Stage => self.workspace_stage(ui),
            Pane::Media => {
                self.media_panel(ui);
            }
            Pane::Preview => self.deck_view(
                ui,
                &tr("미리보기"),
                s.preview.as_ref(),
                false,
                None,
                s,
                acts,
            ),
            Pane::Live => self.deck_view(
                ui,
                &tr("LIVE"),
                s.live.as_ref().map(|l| &l.deck),
                true,
                s.live.as_ref(),
                s,
                acts,
            ),
        }
    }
}
