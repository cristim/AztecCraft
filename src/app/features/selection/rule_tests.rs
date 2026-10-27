use super::*;
use crate::app::tests::{deep_group_editor, remote};

fn select_all(editor: &mut Editor) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::SelectByRule(SelectRule::All))
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
fn select_all_in_component_modes_is_refused_and_keeps_state() {
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
            let before = (editor.selection.clone(), editor.status.clone());
            let operation = Operation::SelectByRule(SelectRule::All);
            let result = if native {
                editor.apply_operation(operation)
            } else {
                remote(&mut editor, operation)
            };
            assert!(
                matches!(result, Err(OperationError::InvalidInput(ref text))
                    if text == "Selection changes need Object or Hierarchy selection mode"),
                "{mode:?} native={native}: {result:?}"
            );
            assert_eq!(editor.selection, before.0);
            assert_eq!(editor.selection_mode, mode);
        }
    }
}

#[test]
fn select_all_on_an_empty_scene_is_unchanged() {
    let mut editor = Editor::default();
    assert!(!changed(select_all(&mut editor)));
    assert!(editor.selection.is_empty());
}
