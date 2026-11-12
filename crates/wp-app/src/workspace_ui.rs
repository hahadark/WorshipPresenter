//! 네 패널의 분할 도킹. 코어 잠금은 설정 복사/반영 때만 잡는다.
use crate::app::{Act, App, Snap};
use eframe::egui::{self, Rect, Sense, UiBuilder};
use serde_json::Value;
use wp_core::workspace::{Node, Pane, Workspace};
pub struct WorkspaceUi {
    pub open: bool,
    pub state: Workspace,
    name: String,
    persisted: Value,
    pending: Option<(Pane, Pane, u8)>,
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
        }
    }
}
impl App {
    pub(crate) fn workspace_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("workspace-toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("작업공간");
                let current = self
                    .workspace
                    .state
                    .saved
                    .iter()
                    .find(|s| s.id == self.workspace.state.active)
                    .map(|s| s.name.as_str())
                    .unwrap_or("기본")
                    .to_string();
                let mut pick = None;
                egui::ComboBox::from_id_salt("workspace-select")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(self.workspace.state.active == "b-default", "기본")
                            .clicked()
                        {
                            pick = Some("b-default".to_string())
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
                if ui.button("저장 · 관리 · 패널").clicked() {
                    self.workspace.open = true
                }
                ui.checkbox(&mut self.workspace.state.locked, "배치 잠금");
            });
        });
        let mut open = self.workspace.open;
        egui::Window::new("작업공간 · 패널 배치").open(&mut open).show(ui.ctx(),|ui|{
   ui.label("제목을 끌어 다른 패널의 좌·우·위·아래에 놓습니다. 경계선을 끌면 크기가 바뀝니다.");
   ui.add_enabled_ui(!self.workspace.state.locked,|ui|{ui.horizontal_wrapped(|ui|{for p in Pane::ALL{let mut visible=self.workspace.state.visible(p);if ui.checkbox(&mut visible,p.title()).changed(){self.workspace.state.toggle(p)}}});
   if ui.button("기본 배치로 복원").clicked(){self.workspace.state.select("b-default")}});
   ui.separator();ui.horizontal(|ui|{ui.text_edit_singleline(&mut self.workspace.name);if ui.button("지금 배치로 새로 저장").clicked(){self.workspace.state.save_as(&self.workspace.name);self.workspace.name.clear();}});
   let mut use_id=None;let mut delete=None;let layout=self.workspace.state.layout.clone();
   for s in &mut self.workspace.state.saved{ui.horizontal(|ui|{ui.text_edit_singleline(&mut s.name);if ui.button("선택").clicked(){use_id=Some(s.id.clone())}if ui.button("배치 덮어쓰기").clicked(){s.layout=layout.clone()}if ui.button("삭제").clicked(){delete=Some(s.id.clone())}});}
   if let Some(id)=use_id{self.workspace.state.select(&id)}if let Some(id)=delete{self.workspace.state.delete(&id)}
   ui.separator();ui.label("Electron 배치는 원본을 보존하며 Rust 배치는 별도 저장합니다. 탭 합치기·떠 있는 창·접기는 아직 지원하지 않습니다.");
  });
        self.workspace.open = open;
    }
    pub(crate) fn workspace_content(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        // 송출 조작은 패널을 모두 숨겨도 접근할 수 있다.
        egui::Panel::bottom("workspace-controls")
            .show(ui, |ui| self.workspace_controls(ui, s, acts));
        let mut layout = self.workspace.state.layout.take();
        let rect = ui.available_rect_before_wrap();
        if let Some(n) = &mut layout {
            self.workspace_node(ui, rect, n, s, acts, 1);
        } else {
            ui.label("표시된 패널이 없습니다. 위쪽 ‘저장 · 관리 · 패널’에서 다시 표시하세요.");
        }
        self.workspace.state.layout = layout;
        if let Some((p, t, side)) = self.workspace.pending.take() {
            self.workspace.state.move_to(p, t, side)
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
    ) {
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
                let at = (len - 6.0).max(0.0) * *ratio;
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
                        *ratio = (offset / (len - 6.0).max(1.0)).clamp(0.08, 0.92);
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
                self.workspace_node(ui, ra, a, s, acts, id * 2);
                self.workspace_node(ui, rb, b, s, acts, id * 2 + 1);
            }
            Node::Pane(p) => {
                let p = *p;
                ui.scope_builder(
                    UiBuilder::new().max_rect(rect).id_salt(("pane", id)),
                    |ui| {
                        ui.set_clip_rect(rect);
                        ui.set_min_size(rect.size());
                        ui.horizontal(|ui| {
                            let r = ui.add(egui::Label::new(p.title()).sense(
                                if self.workspace.state.locked {
                                    Sense::hover()
                                } else {
                                    Sense::drag()
                                },
                            ));
                            if !self.workspace.state.locked {
                                r.dnd_set_drag_payload(p);
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
                                        let side = if x.min(1.0 - x) < y.min(1.0 - y) {
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
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .id_salt(("body", id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| match p {
                                Pane::Cue => self.workspace_cue(ui, s, acts),
                                Pane::Bible => self.search_panel(ui, s, acts),
                                Pane::Preview => self.deck_view(
                                    ui,
                                    "미리보기",
                                    s.preview.as_ref(),
                                    &s.tpl_main,
                                    None,
                                    s,
                                    acts,
                                ),
                                Pane::Live => self.deck_view(
                                    ui,
                                    "LIVE (예배당)",
                                    s.live.as_ref().map(|l| &l.deck),
                                    &s.tpl_main,
                                    s.live.as_ref(),
                                    s,
                                    acts,
                                ),
                            });
                    },
                );
            }
        }
    }
}
