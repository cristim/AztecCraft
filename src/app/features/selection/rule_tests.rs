use super::*;
use crate::app::tests::{deep_group_editor, remote};

fn select_by(editor: &mut Editor, rule: SelectRule) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::SelectByRule(rule))
}

fn select_all(editor: &mut Editor) -> Result<OperationReply, OperationError> {
    select_by(editor, SelectRule::All)
}

fn select_only(editor: &mut Editor, target: &TransformTarget) {
    editor
        .apply_operation(Operation::Select {
            node: Some(target.node),
            instance_path: Some(target.path.clone()),
        })
        .unwrap();
}

/// Three flat cubes, nothing selected.
fn three_cubes() -> (Editor, Vec<TransformTarget>) {
    let mut editor = Editor::default();
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    editor.deselect_all();
    let cubes = editor.history.scene().transform_paths().unwrap();
    assert_eq!(cubes.len(), 3);
    (editor, cubes)
}

fn changed(reply: Result<OperationReply, OperationError>) -> bool {
    matches!(reply.unwrap(), OperationReply::Changed)
}

#[test]
fn select_all_in_object_mode_selects_nested_instances() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    assert_eq!(all.len(), 3);
    assert!(changed(select_all(&mut editor)));
    assert_eq!(editor.selection.members(), all);
    assert_eq!(editor.selection.lead(), all.last());
    let lead = editor
        .history
        .scene()
        .node(all.last().unwrap().node)
        .unwrap();
    assert_eq!(editor.name, lead.name());
    assert!(!changed(select_all(&mut editor)));
}

#[test]
fn select_all_replaces_an_existing_selection_and_makes_the_last_instance_the_lead() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    let first = all[0].clone();
    editor
        .apply_operation(Operation::Select {
            node: Some(first.node),
            instance_path: Some(first.path.clone()),
        })
        .unwrap();
    assert!(changed(select_all(&mut editor)));
    assert_eq!(editor.selection.members(), all);
    assert_eq!(editor.selection.lead(), all.last());
}

#[test]
fn select_all_in_hierarchy_mode_selects_roots_only() {
    let mut editor = deep_group_editor();
    editor.selection_mode = SelectionMode::Hierarchy;
    let roots: Vec<_> = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .filter(|target| target.path.len() == 1)
        .collect();
    assert_eq!(roots.len(), 1);
    assert!(changed(select_all(&mut editor)));
    assert_eq!(editor.selection.members(), roots);
    assert_eq!(editor.selection_mode, SelectionMode::Hierarchy);
}

#[test]
fn select_rules_in_component_modes_are_refused_and_keep_state() {
    for rule in [SelectRule::All, SelectRule::Hierarchy, SelectRule::Inverse] {
        for mode in [
            SelectionMode::Face,
            SelectionMode::Vertex,
            SelectionMode::Edge,
        ] {
            for native in [true, false] {
                let mut editor = deep_group_editor();
                let one = editor.history.scene().transform_paths().unwrap().remove(0);
                editor.set_selection_members_for_test(vec![one]);
                editor.sync_fields();
                editor.selection_mode = mode;
                let before = editor.selection.clone();
                let operation = Operation::SelectByRule(rule);
                let result = if native {
                    editor.apply_operation(operation)
                } else {
                    remote(&mut editor, operation)
                };
                assert!(
                    matches!(result, Err(OperationError::InvalidInput(ref text))
                        if text == "Selection changes need Object or Hierarchy selection mode"),
                    "{rule:?} {mode:?} native={native}: {result:?}"
                );
                assert_eq!(editor.selection, before);
                assert_eq!(editor.selection_mode, mode);
            }
        }
    }
}

#[test]
fn select_all_on_an_empty_scene_is_unchanged() {
    let mut editor = Editor::default();
    assert!(!changed(select_all(&mut editor)));
    assert!(editor.selection.is_empty());
}

#[test]
fn select_hierarchy_adds_descendants_keeps_lead() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    select_only(&mut editor, &all[0]);
    assert!(changed(select_by(&mut editor, SelectRule::Hierarchy)));
    assert_eq!(
        editor.selection.members(),
        [all[1].clone(), all[2].clone(), all[0].clone()]
    );
    assert_eq!(editor.selection.lead(), Some(&all[0]));
    assert!(!changed(select_by(&mut editor, SelectRule::Hierarchy)));
}

#[test]
fn select_hierarchy_keeps_other_members_in_place_and_adds_only_new_descendants() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(vec![all[0].clone(), all[2].clone()]);
    editor.sync_fields();
    assert!(changed(select_by(&mut editor, SelectRule::Hierarchy)));
    assert_eq!(
        editor.selection.members(),
        [all[0].clone(), all[1].clone(), all[2].clone()]
    );
    assert_eq!(editor.selection.lead(), Some(&all[2]));
}

#[test]
fn select_hierarchy_of_an_empty_selection_or_a_leaf_is_unchanged() {
    let mut editor = deep_group_editor();
    assert!(!changed(select_by(&mut editor, SelectRule::Hierarchy)));
    assert!(editor.selection.is_empty());
    let leaf = editor.history.scene().transform_paths().unwrap().remove(2);
    select_only(&mut editor, &leaf);
    assert!(!changed(select_by(&mut editor, SelectRule::Hierarchy)));
}

#[test]
fn select_hierarchy_in_hierarchy_mode_is_unchanged() {
    let mut editor = deep_group_editor();
    let root = editor.history.scene().transform_paths().unwrap().remove(0);
    select_only(&mut editor, &root);
    editor.selection_mode = SelectionMode::Hierarchy;
    assert!(!changed(select_by(&mut editor, SelectRule::Hierarchy)));
    assert_eq!(editor.selection.members(), [root]);
}

#[test]
fn select_inverse_swaps_selected_and_unselected() {
    let (mut editor, cubes) = three_cubes();
    select_only(&mut editor, &cubes[0]);
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert_eq!(
        editor.selection.members(),
        [cubes[1].clone(), cubes[2].clone()]
    );
    assert_eq!(editor.selection.lead(), Some(&cubes[2]));
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert_eq!(editor.selection.members(), [cubes[0].clone()]);
}

#[test]
fn select_inverse_of_an_empty_selection_selects_everything() {
    let (mut editor, cubes) = three_cubes();
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert_eq!(editor.selection.members(), cubes);
    assert_eq!(editor.selection.lead(), cubes.last());
}

#[test]
fn select_inverse_of_everything_clears_and_of_an_empty_scene_is_unchanged() {
    let (mut editor, _) = three_cubes();
    select_all(&mut editor).unwrap();
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert!(editor.selection.is_empty());
    let mut empty = Editor::default();
    assert!(!changed(select_by(&mut empty, SelectRule::Inverse)));
}

#[test]
fn select_inverse_in_hierarchy_mode_swaps_roots_only() {
    let mut editor = deep_group_editor();
    editor.selection_mode = SelectionMode::Hierarchy;
    let root = editor.history.scene().transform_paths().unwrap().remove(0);
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert_eq!(editor.selection.members(), [root]);
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert!(editor.selection.is_empty());
}

#[test]
fn select_inverse_in_object_mode_includes_nested_instances() {
    let mut editor = deep_group_editor();
    let all = editor.history.scene().transform_paths().unwrap();
    select_only(&mut editor, &all[1]);
    assert!(changed(select_by(&mut editor, SelectRule::Inverse)));
    assert_eq!(editor.selection.members(), [all[0].clone(), all[2].clone()]);
}
