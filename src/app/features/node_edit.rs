use crate::app::*;

impl Editor {
    /// A remembered component is stale unless it still belongs to the selected instance and resolves.
    fn stale_component(
        &self,
        transform: NodeId,
        path: &[NativeId],
        resolves: impl FnOnce() -> bool,
    ) -> bool {
        let (selected, selected_path) = self.selection_presentation();
        Some(transform) != selected
            || path
                .split_last()
                .is_none_or(|(_, owner)| owner != selected_path)
            || !resolves()
    }
    pub(in crate::app) fn sync_fields(&mut self) {
        self.material_draft = None;
        self.channel_intents = [false; 9];
        self.name_intent = false;
        self.normalize_attribute_editor_tab();
        let Some(node) = self
            .selection_presentation()
            .0
            .and_then(|id| self.history.scene().node(id).ok())
        else {
            self.selected = None;
            self.selected_path.clear();
            self.clear_remembered_components();
            self.name.clear();
            self.channels = Default::default();
            self.material_shape = None;
            return;
        };
        let scene = self.history.scene();
        let stale_face = self.remembered_face.as_ref().is_some_and(|target| {
            self.stale_component(target.transform, &target.path, || {
                scene.resolve_face_target(target).is_ok()
            })
        });
        let stale_vertex = self.remembered_vertex.as_ref().is_some_and(|target| {
            self.stale_component(target.transform, &target.path, || {
                scene.resolve_vertex_target(target).is_ok()
            })
        });
        let stale_edge = self.remembered_edge.as_ref().is_some_and(|target| {
            self.stale_component(target.transform, &target.path, || {
                scene.resolve_edge_target(target).is_ok()
            })
        });
        self.name = node.name().into();
        let values = node.transform().values();
        self.channels = values.map(|value| value.to_string());
        let transform = node.transform_id();
        if stale_face {
            self.remembered_face = None;
        }
        if stale_vertex {
            self.remembered_vertex = None;
        }
        if stale_edge {
            self.remembered_edge = None;
        }
        if !self
            .material_shape
            .is_some_and(|shape| self.is_direct_shape(transform, shape))
        {
            let mut shapes = self
                .history
                .scene()
                .dag_edges()
                .iter()
                .filter(|edge| edge.parent() == Some(transform))
                .filter_map(|edge| self.history.scene().shape(edge.child()).ok())
                .map(|shape| shape.id());
            self.material_shape = shapes.next().filter(|_| shapes.next().is_none());
        }
        if let Some(shape) = self.material_shape
            && let Ok(value) = self.history.scene().material_for_shape(shape)
        {
            self.material_draft = Some(MaterialDraft {
                scene_token: self.scene_token,
                shape,
                original: value.diffuse,
                text: value.diffuse.to_string(),
                intent: false,
                focus: material_edit_id(),
            });
        }
    }
    pub(in crate::app) fn create(&mut self, primitive: Primitive) {
        self.native_operation(Operation::Create {
            primitive,
            options: Some(self.remembered_options(primitive)),
            name: None,
        });
    }
    pub(in crate::app) fn appearance_scope(
        &self,
    ) -> Result<
        (&'static str, azteccraft::native_scene::AppearanceSummary),
        azteccraft::native_scene::NativeError,
    > {
        if let Some(id) = self.selection_presentation().0
            && let Some(summary) = self.history.scene().transform_appearance_readiness(id)?
        {
            return Ok(("Direct shapes", summary));
        }
        Ok(("Scene totals", self.history.scene().appearance_readiness()))
    }
    pub(in crate::app) fn appearance_summary(&self, ui: &mut egui::Ui) {
        let Ok((scope, summary)) = self.appearance_scope() else {
            ui.weak("Appearance source state unavailable");
            return;
        };
        ui.label(egui::RichText::new(format!("{scope} ({})", summary.shapes)).small());
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!(
                    "Source bindings: {} checked, {} unavailable, {} unbound",
                    summary.checked_source_bindings, summary.unavailable, summary.unbound
                ))
                .small(),
            )
            .wrap(),
        );
        let issues = summary.issues;
        let reasons = [
            (issues.missing_membership, "missing membership"),
            (issues.overlapping_membership, "overlapping membership"),
            (issues.unsupported_shader, "unsupported shader"),
            (issues.unsupported_graph, "unsupported graph"),
            (issues.ambiguous_instance, "ambiguous instance"),
            (issues.unsupported_settings, "unsupported settings"),
            (issues.uv_mismatch, "UV mismatch"),
            (issues.malformed_data, "malformed data"),
        ]
        .into_iter()
        .filter_map(|(present, label)| present.then_some(label))
        .collect::<Vec<_>>()
        .join(", ");
        if !reasons.is_empty() {
            ui.add(egui::Label::new(egui::RichText::new(reasons).small()).wrap());
        }
        ui.add(
            egui::Label::new(
                egui::RichText::new(
                    "Evaluation unavailable; filtering unproved; no color processor.",
                )
                .weak()
                .small(),
            )
            .wrap(),
        );
    }
    /// The object-name draft field. `Heading` is the Channel Box section heading; `Plain` is the
    /// labelled form row. Both share one widget id and draft. Returns `lost_focus` so the caller
    /// folds it into its single blur commit.
    pub(in crate::app) fn name_field(
        &mut self,
        ui: &mut egui::Ui,
        authoring: bool,
        style: NameStyle,
    ) -> bool {
        let reason = self.read_only_reason();
        let edit = egui::TextEdit::singleline(&mut self.name)
            .interactive(authoring)
            .id(name_edit_id());
        let response = match style {
            NameStyle::Plain => {
                ui.label("Object name");
                ui.add_enabled(authoring, edit)
            }
            NameStyle::Heading => {
                let response = ui.add_enabled(
                    authoring,
                    edit.font(egui::TextStyle::Heading)
                        .frame(egui::Frame::NONE)
                        .desired_width(f32::INFINITY),
                );
                ui.ctx().accesskit_node_builder(response.id, |node| {
                    node.set_label("Object name");
                });
                response
            }
        };
        let response = match &reason {
            Some(reason) => response.on_disabled_hover_text(reason),
            None => response,
        };
        self.name_intent |= response.changed();
        ui.separator();
        response.lost_focus()
    }
    /// One blur commit per frame, shared by the name field and the channel fields.
    pub(in crate::app) fn commit_on_blur(&mut self, apply: bool) {
        if apply && self.blur_commit_skip_frames == 0 {
            self.commit_pending();
        }
    }
    /// The nine transform channels and the read-only Visibility row, shared by the Channel Box and
    /// the Attribute Editor Transform tab so both edit the same drafts and widget ids. Returns
    /// whether a channel field lost focus.
    pub(in crate::app) fn transform_section(&mut self, ui: &mut egui::Ui, authoring: bool) -> bool {
        let reason = self.read_only_reason();
        let mut apply = false;
        let visibility = self
            .selection_presentation()
            .0
            .and_then(|id| self.history.scene().node(id).ok())
            .and_then(|node| node.local_visibility());
        egui::Grid::new("transform_channels")
            .num_columns(2)
            .spacing([8.0, 5.0])
            .show(ui, |ui| {
                for (index, (label, value)) in [
                    "Translate X",
                    "Translate Y",
                    "Translate Z",
                    "Rotate X",
                    "Rotate Y",
                    "Rotate Z",
                    "Scale X",
                    "Scale Y",
                    "Scale Z",
                ]
                .iter()
                .zip(&mut self.channels)
                .enumerate()
                {
                    ui.label(*label);
                    let response = ui.add_enabled(
                        authoring,
                        egui::TextEdit::singleline(value)
                            .interactive(authoring)
                            .id(channel_edit_id(index))
                            .desired_width(100.0),
                    );
                    let response = match &reason {
                        Some(reason) => response.on_disabled_hover_text(reason),
                        None => response,
                    };
                    self.channel_intents[index] |= response.changed();
                    apply |= response.lost_focus();
                    ui.end_row();
                }
                ui.label("Visibility");
                ui.add_enabled(false, egui::Button::new(visibility_text(visibility)))
                    .on_disabled_hover_text("visibility edit not implemented");
                ui.end_row();
            });
        let angular = match &self.history.scene().units().angular {
            AngularUnit::Degree => "degrees",
            AngularUnit::Radian => "radians",
            AngularUnit::Unsupported(unit) => unit,
        };
        ui.weak(format!("Rotation in {angular} | XYZ"));
        apply
    }
    pub(in crate::app) fn read_only_reason(&self) -> Option<String> {
        self.history
            .scene()
            .authoring_read_only_reason()
            .map(|reason| reason.to_string())
    }
    /// Help Line row under the time slider: the read-only reason and curves note that the blank
    /// Channel Box no longer carries, and the appearance summary behind a closed header.
    pub(in crate::app) fn help_line(&self, ui: &mut egui::Ui) {
        if let Some(reason) = self.read_only_reason() {
            ui.weak(reason);
        }
        if !self.history.scene().curves().is_empty() {
            ui.weak("Curves: select in the Outliner and frame with F. Viewport curve picking and CV editing are unavailable.");
        }
        egui::CollapsingHeader::new("Scene diagnostics")
            .id_salt("scene_diagnostics")
            .default_open(false)
            .show(ui, |ui| self.appearance_summary(ui));
    }
    pub(in crate::app) fn channels(&mut self, ui: &mut egui::Ui, delete_intent: &mut bool) {
        ui.label(RichText::new("Channel Box").strong());
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("channel_box_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| self.channel_sections(ui, delete_intent));
    }

    fn channel_sections(&mut self, ui: &mut egui::Ui, delete_intent: &mut bool) {
        if self.single_target().is_err() {
            unsupported_section(ui, "Layers");
            return;
        }
        let read_only_reason = self.read_only_reason();
        let authoring = read_only_reason.is_none();
        let name_blur = self.name_field(ui, authoring, NameStyle::Heading);
        let channel_blur = self.transform_section(ui, authoring);
        self.commit_on_blur(name_blur | channel_blur);
        for heading in ["SHAPES", "INPUTS", "OUTPUTS"] {
            unsupported_section(ui, heading);
        }
        self.material_controls(ui);
        ui.separator();
        if ui
            .add_enabled(
                authoring && !self.selection_mode.is_component(),
                egui::Button::new("Delete selected"),
            )
            .on_disabled_hover_text(self.delete_disabled_hover(read_only_reason.as_deref(), false))
            .clicked()
        {
            *delete_intent = true;
        }
        ui.separator();
        unsupported_section(ui, "Layers");
    }
}

fn unsupported_section(ui: &mut egui::Ui, heading: &str) {
    ui.label(RichText::new(heading).strong());
    let response = unsupported(ui, "Not implemented yet");
    ui.ctx().accesskit_node_builder(response.id, |node| {
        node.set_label(format!("{heading}: not implemented yet"));
    });
}

#[derive(Clone, Copy)]
pub(in crate::app) enum NameStyle {
    Heading,
    Plain,
}

fn visibility_text(visibility: Option<bool>) -> &'static str {
    match visibility {
        Some(true) => "on",
        Some(false) => "off",
        None => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::visibility_text;

    #[test]
    fn unproved_visibility_reads_unknown_and_never_defaults_to_on() {
        assert_eq!(visibility_text(None), "unknown");
        assert_eq!(visibility_text(Some(true)), "on");
        assert_eq!(visibility_text(Some(false)), "off");
    }
}
