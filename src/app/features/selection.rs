use crate::app::*;

impl Editor {
    pub(in crate::app) fn selection_presentation(&self) -> (Option<NodeId>, &[NativeId]) {
        (self.selected, self.selected_path.as_slice())
    }

    pub(in crate::app) fn single_target(&self) -> Result<(NodeId, &[NativeId]), OperationError> {
        let (selected, path) = self.selection_presentation();
        Ok((selected.ok_or(OperationError::NoSelection)?, path))
    }

    /// Select > Deselect All and Alt+D share this one native caller of `DeselectAll` (#167).
    pub(in crate::app) fn deselect_all(&mut self) {
        self.native_operation(Operation::DeselectAll);
    }

    #[cfg(test)]
    pub(in crate::app) fn select(&mut self, selected: Option<NodeId>) {
        self.native_operation(Operation::Select {
            node: selected,
            instance_path: None,
        });
    }
    pub(in crate::app) fn selection_path(
        &self,
        id: NodeId,
        provided: Option<Vec<NativeId>>,
    ) -> Result<Vec<NativeId>, OperationError> {
        self.history
            .scene()
            .node(id)
            .map_err(OperationError::Scene)?;
        if let Some(path) = provided {
            if path.len() > azteccraft::native_scene::MAX_TRANSFORM_DEPTH {
                return Err(OperationError::InvalidInput(
                    "Instance path is too deep".into(),
                ));
            }
            self.history
                .scene()
                .resolve_transform_path(id, &path)
                .map_err(OperationError::Scene)?;
            return Ok(path);
        }
        let mut matches = self
            .history
            .scene()
            .transform_paths()
            .map_err(OperationError::Scene)?
            .into_iter()
            .filter(|target| target.node == id);
        let first = matches
            .next()
            .ok_or_else(|| OperationError::InvalidInput("Object has no instance path".into()))?;
        if matches.next().is_some() {
            return Err(OperationError::AmbiguousSelection);
        }
        Ok(first.path)
    }
    pub(in crate::app) fn outliner(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Outliner").strong());
        ui.separator();
        ui.weak("Scene root");
        let paths = match self.history.scene().transform_paths() {
            Ok(paths) => paths,
            Err(error) => {
                ui.colored_label(Color32::LIGHT_RED, error.to_string());
                return;
            }
        };
        let mut choice = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for target in paths {
                let Ok(node) = self.history.scene().node(target.node) else {
                    continue;
                };
                ui.horizontal(|ui| {
                    ui.add_space((target.path.len() - 1) as f32 * 12.0);
                    let (selected, path) = self.selection_presentation();
                    if ui
                        .selectable_label(
                            selected == Some(target.node) && path == target.path,
                            format!("◇  {}", node.name()),
                        )
                        .clicked()
                    {
                        choice = Some(target);
                    }
                });
            }
        });
        if let Some(target) = choice {
            self.native_operation(Operation::Select {
                node: Some(target.node),
                instance_path: Some(target.path),
            });
        }
        if self.history.scene().nodes().is_empty() {
            ui.weak("No objects. Create a primitive above.");
        }
    }
}
