use crate::app::*;

impl Editor {
    pub(in crate::app) fn selection_presentation(&self) -> (Option<NodeId>, &[NativeId]) {
        self.selection.lead().map_or((None, &[][..]), |lead| {
            (Some(lead.node), lead.path.as_slice())
        })
    }

    /// The lead, for display and drafts; with several objects selected it is still the lead.
    pub(in crate::app) fn lead_target(&self) -> Result<(NodeId, &[NativeId]), OperationError> {
        let (selected, path) = self.selection_presentation();
        Ok((selected.ok_or(OperationError::NoSelection)?, path))
    }

    /// The one selected object; refuses an empty or a multiple selection.
    pub(in crate::app) fn single_target(&self) -> Result<(NodeId, &[NativeId]), OperationError> {
        if self.selection.len() > 1 {
            return Err(OperationError::MultipleSelection);
        }
        self.lead_target()
    }

    #[cfg(test)]
    pub(in crate::app) fn selected(&self) -> Option<NodeId> {
        self.selection_presentation().0
    }

    #[cfg(test)]
    pub(in crate::app) fn selected_path(&self) -> &[NativeId] {
        self.selection_presentation().1
    }

    /// Fixture setter: the one test entry for building a selection without dispatching `Select`.
    #[cfg(test)]
    pub(in crate::app) fn set_selection_for_test(
        &mut self,
        node: Option<NodeId>,
        path: Vec<NativeId>,
    ) {
        match node {
            Some(node) => self.selection.replace(TransformTarget { node, path }),
            None => self.selection.clear(),
        }
    }

    /// Fixture setter for a multi-object selection; the last target is the lead.
    #[cfg(test)]
    pub(in crate::app) fn set_selection_members_for_test(&mut self, members: Vec<TransformTarget>) {
        self.selection.clear();
        for member in members {
            self.selection.add(member).unwrap();
        }
    }

    /// Replaces the object selection by `rule`. Hierarchy mode (Root submask) only ever holds
    /// roots, so its rules see only the root transforms. PROVISIONAL, pinned by
    /// `select_all_in_hierarchy_mode_selects_roots_only` and its Inverse and Hierarchy twins.
    pub(in crate::app) fn select_by_rule(
        &mut self,
        rule: SelectRule,
    ) -> Result<OperationReply, OperationError> {
        let scene = self.history.scene();
        let roots_only = self.selection_mode == SelectionMode::Hierarchy;
        let mut candidates = scene.transform_paths().map_err(OperationError::Scene)?;
        if roots_only {
            candidates.retain(|target| target.path.len() == 1);
        }
        let members = match rule {
            SelectRule::All => candidates,
            SelectRule::Inverse => {
                candidates.retain(|target| !self.selection.contains(target));
                candidates
            }
            // Added nodes sit before the lead so it stays the lead. Roots have no descendants
            // to add in Hierarchy mode.
            SelectRule::Hierarchy => {
                let current = self.selection.members();
                let added = if roots_only {
                    Vec::new()
                } else {
                    scene
                        .descendant_targets(current)
                        .map_err(OperationError::Scene)?
                        .into_iter()
                        .filter(|target| !self.selection.contains(target))
                        .collect()
                };
                match current.split_last() {
                    Some((lead, rest)) => rest
                        .iter()
                        .cloned()
                        .chain(added)
                        .chain(std::iter::once(lead.clone()))
                        .collect(),
                    None => Vec::new(),
                }
            }
        };
        let mut next = self.selection.clone();
        next.replace_with(members)
            .map_err(|error| OperationError::InvalidInput(error.to_string()))?;
        let changed = next != self.selection;
        if changed {
            self.selection = next;
            self.clear_remembered_components();
            self.sync_fields();
        }
        Ok(if changed {
            OperationReply::Changed
        } else {
            OperationReply::Unchanged
        })
    }

    /// The Select menu rows and Ctrl/Cmd+Shift+A and +I share this one native caller of
    /// `SelectByRule`.
    pub(in crate::app) fn native_select_by_rule(&mut self, rule: SelectRule) {
        self.native_operation(Operation::SelectByRule(rule));
    }

    /// Select > Deselect All and Alt+D share this one native caller of `DeselectAll`.
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
        only_instance_path(
            id,
            self.history
                .scene()
                .transform_paths()
                .map_err(OperationError::Scene)?,
        )
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
                    let label = format!("◇  {}", node.name());
                    // With several selected, the lead row is bold (PROVISIONAL; the page gives
                    // no Outliner colour for the key object).
                    let text = if self.selection.len() > 1 && self.selection.lead() == Some(&target)
                    {
                        RichText::new(label).strong()
                    } else {
                        RichText::new(label)
                    };
                    let row = ui.selectable_label(self.selection.contains(&target), text);
                    if row.clicked() {
                        choice = Some((target, ui.input(|input| input.modifiers)));
                    }
                });
            }
        });
        if let Some((target, modifiers)) = choice {
            // The 2027 Outliner page: Shift adds to the selection and Ctrl toggles. Shift wins
            // when both are held (PROVISIONAL). Unlike the viewport, Ctrl on an unselected row
            // selects it.
            let change = if modifiers.shift {
                Some(SelectionChange::Add)
            } else if modifiers.ctrl || modifiers.command {
                Some(SelectionChange::Toggle)
            } else {
                None
            };
            let (node, instance_path) = (target.node, Some(target.path));
            self.native_operation(match change {
                Some(change) => Operation::ModifySelection {
                    change,
                    node,
                    instance_path,
                },
                None => Operation::Select {
                    node: Some(node),
                    instance_path,
                },
            });
        }
        if self.history.scene().nodes().is_empty() {
            ui.weak("No objects. Create a primitive above.");
        }
    }
}

/// The one instance path of `id` among `targets`; several paths need the caller to pick one.
pub(in crate::app) fn only_instance_path(
    id: NodeId,
    targets: Vec<TransformTarget>,
) -> Result<Vec<NativeId>, OperationError> {
    let mut matches = targets.into_iter().filter(|target| target.node == id);
    let first = matches
        .next()
        .ok_or_else(|| OperationError::InvalidInput("Object has no instance path".into()))?;
    if matches.next().is_some() {
        return Err(OperationError::AmbiguousSelection);
    }
    Ok(first.path)
}

#[cfg(test)]
mod delete_tests;

#[cfg(test)]
mod rule_tests;

#[cfg(test)]
mod tests {
    use super::*;

    /// No decoder or constructor yields one transform under two parents, so the instanced
    /// transform is fabricated here: a second target with the same node and another path.
    #[test]
    fn an_object_with_two_instance_paths_needs_an_explicit_path() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        editor.create(Primitive::Cube);
        let targets = editor.history.scene().transform_paths().unwrap();
        let (first, second) = (targets[0].clone(), targets[1].clone());
        let instanced = vec![
            first.clone(),
            TransformTarget {
                node: first.node,
                path: second.path.clone(),
            },
            second.clone(),
        ];
        assert!(matches!(
            only_instance_path(first.node, instanced.clone()),
            Err(OperationError::AmbiguousSelection)
        ));
        assert_eq!(
            only_instance_path(second.node, instanced.clone()).unwrap(),
            second.path
        );
        assert!(matches!(
            only_instance_path(first.node, vec![first.clone()]),
            Ok(path) if path == first.path
        ));
        assert!(matches!(
            only_instance_path(first.node, Vec::new()),
            Err(OperationError::InvalidInput(_))
        ));
    }

    #[test]
    fn select_and_modify_selection_without_a_path_resolve_a_single_instance() {
        let mut editor = Editor::default();
        editor.create(Primitive::Cube);
        let target = editor.history.scene().transform_paths().unwrap().remove(0);
        editor.deselect_all();
        for operation in [
            Operation::Select {
                node: Some(target.node),
                instance_path: None,
            },
            Operation::ModifySelection {
                change: SelectionChange::Toggle,
                node: target.node,
                instance_path: None,
            },
        ] {
            editor.apply_operation(operation).unwrap();
            assert_eq!(editor.selection.members().len(), 1);
            editor.deselect_all();
        }
    }
}
