use crate::{app::Editor, editor_operations::Operation};
use azteccraft::native_scene::DisplayLayerEdit;
use eframe::egui;

#[cfg(any(test, feature = "mcp"))]
use crate::editor_operations::{DisplayLayerInspection, OperationReply};

impl Editor {
    pub(in crate::app) fn display_layer_editor(&mut self, ui: &mut egui::Ui) {
        let scene = self.history.scene();
        let sourced = scene.appearance_incomplete();
        let members: Vec<_> = self
            .selection
            .members()
            .iter()
            .map(|target| target.node)
            .collect();
        let can_assign = !self.selection_mode.is_component() && !members.is_empty();
        let mut edit = None;
        ui.heading("Display Layers");
        if sourced {
            ui.label("Display layer editing is unavailable for retained Maya scenes");
        }
        ui.add_enabled_ui(!sourced, |ui| {
            ui.menu_button("Layers", |ui| {
                if ui.button("Create Empty Layer").clicked() {
                    edit = Some(DisplayLayerEdit::Create {
                        members: Vec::new(),
                    });
                    ui.close();
                }
                if ui
                    .add_enabled(can_assign, egui::Button::new("Create Layer from Selected"))
                    .on_disabled_hover_text("Select one or more objects")
                    .clicked()
                {
                    edit = Some(DisplayLayerEdit::Create {
                        members: members.clone(),
                    });
                    ui.close();
                }
                if ui
                    .add_enabled(
                        can_assign,
                        egui::Button::new("Move Selected to Default Layer"),
                    )
                    .on_disabled_hover_text("Select one or more objects")
                    .clicked()
                {
                    edit = Some(DisplayLayerEdit::Assign {
                        layer: 0,
                        members: members.clone(),
                    });
                    ui.close();
                }
            });
            for (id, layer) in scene.display_layers() {
                ui.push_id(id, |ui| {
                    ui.horizontal(|ui| {
                        let mut visible = layer.visible();
                        if ui
                            .checkbox(&mut visible, format!("layer{id} visible"))
                            .changed()
                        {
                            edit = Some(DisplayLayerEdit::Visibility { layer: id, visible });
                        }
                        ui.label(format!("{} members", layer.member_count()));
                        ui.menu_button("Actions", |ui| {
                            if ui
                                .add_enabled(can_assign, egui::Button::new("Add Selected Objects"))
                                .on_disabled_hover_text("Select one or more objects")
                                .clicked()
                            {
                                edit = Some(DisplayLayerEdit::Assign {
                                    layer: id,
                                    members: members.clone(),
                                });
                                ui.close();
                            }
                            if ui.button("Delete Layer").clicked() {
                                edit = Some(DisplayLayerEdit::Delete { layer: id });
                                ui.close();
                            }
                        });
                    });
                });
            }
        });
        if scene.display_layers().next().is_some() {
            ui.label("Display layers are session-only. Delete or undo layers before saving.");
        }
        ui.add_enabled(false, egui::Button::new("Playback, Display Type and Color"))
            .on_disabled_hover_text(
                "Display layer playback, template/reference and color are unsupported",
            );
        ui.add_enabled(false, egui::Button::new("Animation Layers"))
            .on_disabled_hover_text("Animation layers are unsupported");
        if let Some(edit) = edit {
            self.native_operation(Operation::EditDisplayLayer(edit));
        }
    }

    #[cfg(any(test, feature = "mcp"))]
    pub(in crate::app) fn inspect_display_layers(&self) -> OperationReply {
        let scene = self.history.scene();
        let layers = scene
            .display_layers()
            .map(|(id, layer)| DisplayLayerInspection {
                id,
                name: format!("layer{id}"),
                visible: layer.visible(),
                members: scene
                    .nodes()
                    .iter()
                    .filter(|node| layer.contains(node.transform_id()))
                    .map(|node| node.id().to_decimal())
                    .collect(),
            })
            .collect();
        OperationReply::DisplayLayers {
            scene_token: self.scene_token.to_string(),
            layers,
        }
    }
}
