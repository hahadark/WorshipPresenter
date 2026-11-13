//! 여덟 패널의 탭·분할·독립 창 도킹. 코어 잠금은 설정 복사/반영 때만 잡는다.
use crate::app::{Act, App, Snap};
use eframe::egui::{self, Rect, Sense, UiBuilder};
use serde_json::Value;
use wp_core::i18n::tr;
use wp_core::workspace::{Node, Pane, Workspace};
pub struct WorkspaceUi {
    pub open: bool,
    pub state: Workspace,
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
    pub(crate) fn workspace_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("작업공간"));
                let current = self
                    .workspace
                    .state
                    .saved
                    .iter()
                    .find(|s| s.id == self.workspace.state.active)
                    .map(|s| s.name.as_str())
                    .unwrap_or_else(|| {
                        Workspace::BUILTINS
                            .iter()
                            .find(|(id, _)| *id == self.workspace.state.active)
                            .map(|(_, name)| *name)
                            .unwrap_or("기본")
                    })
                    .to_string();
                let mut pick = None;
                egui::ComboBox::from_id_salt("workspace-select")
                    .selected_text(if self.workspace.state.active.starts_with("b-") {
                        tr(&current)
                    } else {
                        current
                    })
                    .show_ui(ui, |ui| {
                        for (id, name) in Workspace::BUILTINS {
                            if ui
                                .selectable_label(self.workspace.state.active == id, tr(name))
                                .clicked()
                            {
                                pick = Some(id.to_string())
                            }
                        }
                        for s in &self.workspace.state.saved {
                            if ui
                                .selectable_label(self.workspace.state.active == s.id, &s.name)
                                .clicked()
                            {
                                pick = Some(s.id.clone())
                            }
                        }
                    });
                if let Some(id) = pick {
                    self.workspace.state.select(&id)
                }
                if ui.button(tr("저장 · 관리 · 패널")).clicked() {
                    self.workspace.open = true
                }
                ui.checkbox(&mut self.workspace.state.locked, tr("배치 잠금"));
            });
        });
        let mut open = self.workspace.open;
        egui::Window::new(tr("작업공간 · 패널 배치")).open(&mut open).show(ui.ctx(),|ui|{
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
        // 송출 조작은 패널을 모두 숨겨도 접근할 수 있다.
        if !self.workspace.state.quick_rendered() {
            egui::Panel::bottom("workspace-controls")
                .show(ui, |ui| self.workspace_controls(ui, s, acts));
        }
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
        for f in self.workspace.state.floating.clone() {
            let ctx = ui.ctx().clone();
            let mut attach = false;
            let mut node = f.node();
            let mut geometry = None;
            let mut builder = egui::ViewportBuilder::default()
                .with_title(tr(f.pane.title()))
                .with_inner_size(f.size.unwrap_or([520.0, 420.0]));
            if let Some(pos) = f.position {
                builder = builder.with_position(pos);
            }
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("workspace-float", format!("{:?}", f.pane))),
                builder,
                |ctx, _| {
                    egui::CentralPanel::default_margins().show(ctx, |ui| {
                        if ui
                            .add_enabled(
                                !self.workspace.state.locked,
                                egui::Button::new(tr("제자리에 붙이기")),
                            )
                            .clicked()
                        {
                            attach = true
                        }
                        let rect = ui.available_rect_before_wrap();
                        self.workspace_node(
                            ui,
                            rect,
                            &mut node,
                            s,
                            acts,
                            100 + f.pane as u64,
                            false,
                        );
                    });
                    geometry = ctx.input(|i| {
                        let v = i.viewport();
                        if v.minimized == Some(true) || v.maximized == Some(true) {
                            return None;
                        }
                        let pos = v.outer_rect?.min;
                        let size = v.inner_rect?.size();
                        Some(([pos.x, pos.y], [size.x, size.y]))
                    });
                    if ctx.input(|i| i.viewport().close_requested()) {
                        ctx.ctx()
                            .send_viewport_cmd(egui::ViewportCommand::CancelClose);
                        attach = true
                    }
                },
            );
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
                        saved.position = Some(position);
                        saved.size = Some(size);
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
                    28.0
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
                ui.painter().rect_filled(
                    divider,
                    0.0,
                    if response.hovered() {
                        ui.visuals().selection.bg_fill
                    } else {
                        ui.visuals().widgets.noninteractive.bg_fill
                    },
                );
                self.workspace_node(ui, ra, a, s, acts, id * 2, !*horizontal);
                self.workspace_node(ui, rb, b, s, acts, id * 2 + 1, !*horizontal);
            }
            Node::Pane(_) => unreachable!(),
            Node::Tabs {
                panes,
                active,
                collapsed,
            } => {
                let p = *active;
                ui.scope_builder(
                    UiBuilder::new().max_rect(rect).id_salt(("pane", id)),
                    |ui| {
                        ui.set_clip_rect(rect);
                        ui.set_min_size(rect.size());
                        ui.horizontal_wrapped(|ui| {
                            for tab in panes.iter().copied() {
                                let r = ui.add(
                                    egui::Button::new(tr(tab.title()))
                                        .selected(*active == tab)
                                        .sense(if self.workspace.state.locked {
                                            Sense::click()
                                        } else {
                                            Sense::click_and_drag()
                                        }),
                                );
                                if r.clicked() {
                                    *active = tab;
                                    *collapsed = false
                                }
                                if !self.workspace.state.locked {
                                    r.dnd_set_drag_payload(tab);
                                }
                                r.context_menu(|ui| {
                                    ui.add_enabled_ui(!self.workspace.state.locked, |ui| {
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
                                    });
                                });
                            }
                            ui.add_enabled_ui(!self.workspace.state.locked, |ui| {
                                ui.menu_button(tr("그룹"), |ui| {
                                    if ui.button(tr("그룹 전체 독립창")).clicked() {
                                        self.workspace.float_group = Some(p);
                                        ui.close();
                                    }
                                    ui.menu_button(tr("그룹 전체 병합"), |ui| {
                                        // 렌더 전에 수집한 대상에서 자기 그룹을 제외한다.
                                        for target in self.workspace.targets.clone() {
                                            if !panes.contains(&target) {
                                                if ui.button(tr(target.title())).clicked() {
                                                    self.workspace.merge_group = Some((p, target));
                                                    ui.close();
                                                }
                                            }
                                        }
                                    });
                                });
                            });
                            if ui
                                .add_enabled(
                                    !self.workspace.state.locked && (foldable || *collapsed),
                                    egui::Button::new(tr(if *collapsed {
                                        "펼치기"
                                    } else {
                                        "접기"
                                    })),
                                )
                                .clicked()
                            {
                                *collapsed = !*collapsed
                            }
                        });
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
                                            egui::Color32::from_rgba_unmultiplied(80, 140, 230, 70),
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
                            ui.separator();
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
            Pane::Main => self.search_panel(ui, s, acts),
            Pane::Quick => self.quick_panel(ui, s, acts),
            Pane::Library => self.library_tabs_panel(ui, acts),
            Pane::Stage => self.workspace_stage(ui),
            Pane::Media => {
                self.library_panel(ui);
                self.video_bar(ui);
            }
            Pane::Preview => self.deck_view(
                ui,
                &tr("미리보기"),
                s.preview.as_ref(),
                &s.tpl_main,
                None,
                s,
                acts,
            ),
            Pane::Live => self.deck_view(
                ui,
                &tr("LIVE (예배당)"),
                s.live.as_ref().map(|l| &l.deck),
                &s.tpl_main,
                s.live.as_ref(),
                s,
                acts,
            ),
        }
    }
}
