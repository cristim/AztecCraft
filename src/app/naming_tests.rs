//! Issue #313 slice 3: `Operation::PrefixHierarchyNames` and `Operation::SearchReplaceNames`
//! through the shared dispatch path; native and remote callers reach the same `Command`.
use super::tests::remote;
use super::transform_ops_tests::{baseline, depth};
use super::*;
use crate::editor_operations::NameScope;
use azteccraft::native_scene::{NamingRefusal, NativeError, ReparentKeep};

fn prefix(text: &str, generation: Option<u64>) -> Operation {
    Operation::PrefixHierarchyNames {
        prefix: text.into(),
        expected_selection_generation: generation,
    }
}

fn replace(scope: NameScope, search: &str, with: &str) -> Operation {
    Operation::SearchReplaceNames {
        scope,
        search: search.into(),
        replace: with.into(),
        match_case: true,
        expected_selection_generation: None,
    }
}

/// Three cubes `a > b > c` plus a lone `d`; only `a` is selected. Returns the editor and the ids.
fn family() -> (Editor, [NodeId; 4]) {
    let mut editor = Editor::default();
    for name in ["a", "b", "c", "d"] {
        editor
            .apply_operation(Operation::Create {
                primitive: Primitive::Cube,
                options: None,
                name: Some(name.into()),
                create_uvs: false,
            })
            .unwrap();
    }
    let paths = editor.history.scene().transform_paths().unwrap();
    let ids: Vec<NodeId> = paths.iter().map(|p| p.node).collect();
    for pair in [(1, 0), (2, 1)] {
        editor
            .apply_operation(Operation::Reparent {
                id: ids[pair.0],
                parent: Some(ids[pair.1]),
                keep: ReparentKeep::Local,
            })
            .unwrap();
    }
    let root = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|p| p.node == ids[0])
        .unwrap();
    editor.set_selection_members_for_test(vec![root]);
    editor.sync_fields();
    (editor, [ids[0], ids[1], ids[2], ids[3]])
}

fn name(editor: &Editor, id: NodeId) -> String {
    editor.history.scene().node(id).unwrap().name().to_owned()
}

#[test]
fn prefix_is_one_named_undo_step_with_an_exact_restore_across_a_later_edit() {
    let (mut editor, [a, b, c, d]) = family();
    let before = editor.history.scene().clone();
    let steps = depth(&mut editor);
    editor.apply_operation(prefix("p_", None)).unwrap();
    assert_eq!(
        editor.history.undo_label().as_deref(),
        Some("Prefix Hierarchy Names")
    );
    assert_eq!(depth(&mut editor), steps + 1);
    assert_eq!(
        [a, b, c, d].map(|id| name(&editor, id)),
        ["p_a", "p_b", "p_c", "d"]
    );
    let named = editor.history.scene().clone();
    editor
        .apply_operation(Operation::Rename {
            id: d,
            name: "later".into(),
        })
        .unwrap();
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &named);
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    assert!(editor.history.redo());
    assert_eq!(editor.history.scene(), &named);
}

#[test]
fn replace_scopes_selected_hierarchy_and_all() {
    let (mut editor, [a, b, c, d]) = family();
    editor
        .apply_operation(replace(NameScope::Selected, "a", "x"))
        .unwrap();
    assert_eq!(
        [a, b, c, d].map(|id| name(&editor, id)),
        ["x", "b", "c", "d"]
    );
    assert_eq!(
        editor.history.undo_label().as_deref(),
        Some("Search and Replace Names")
    );
    editor
        .apply_operation(replace(NameScope::Hierarchy, "b", "y"))
        .unwrap();
    editor
        .apply_operation(replace(NameScope::Hierarchy, "c", "z"))
        .unwrap();
    assert_eq!(
        [a, b, c, d].map(|id| name(&editor, id)),
        ["x", "y", "z", "d"]
    );
    editor.deselect_all();
    editor
        .apply_operation(replace(NameScope::All, "d", "w"))
        .unwrap();
    assert_eq!(name(&editor, d), "w");
}

#[test]
fn native_and_remote_reach_the_same_scene() {
    let (mut native, ids) = family();
    let (mut other, other_ids) = family();
    native.apply_operation(prefix("p_", None)).unwrap();
    assert!(matches!(
        remote(&mut other, prefix("p_", None)),
        Ok(OperationReply::Changed)
    ));
    assert_eq!(
        ids.map(|id| name(&native, id)),
        other_ids.map(|id| name(&other, id))
    );
    assert_eq!(native.history.undo_label(), other.history.undo_label());
}

#[test]
fn refusals_leave_scene_selection_and_history_untouched() {
    let (mut editor, [_, _, _, _]) = family();
    let before = baseline(&editor);
    assert!(matches!(
        editor.apply_operation(prefix("", None)),
        Err(OperationError::Naming(NativeError::InvalidNameEdit(
            NamingRefusal::EmptyPrefix
        )))
    ));
    assert!(matches!(
        editor.apply_operation(prefix("1", None)),
        Err(OperationError::Naming(NativeError::InvalidName))
    ));
    assert!(matches!(
        editor.apply_operation(replace(NameScope::Hierarchy, "a", "d")),
        Err(OperationError::Naming(NativeError::NameCollision))
    ));
    assert!(matches!(
        editor.apply_operation(replace(NameScope::Selected, "", "d")),
        Err(OperationError::Naming(NativeError::InvalidNameEdit(
            NamingRefusal::EmptySearch
        )))
    ));
    assert!(matches!(
        editor.apply_operation(replace(NameScope::Selected, "zz", "d")),
        Err(OperationError::Naming(NativeError::InvalidNameEdit(
            NamingRefusal::NothingToRename
        )))
    ));
    editor.selection_mode = SelectionMode::Face;
    assert!(matches!(
        editor.apply_operation(prefix("p_", None)),
        Err(OperationError::InvalidInput(_))
    ));
    editor.selection_mode = SelectionMode::Object;
    assert!(baseline(&editor) == before);
    editor.deselect_all();
    assert!(matches!(
        editor.apply_operation(prefix("p_", None)),
        Err(OperationError::NoSelection)
    ));
    assert!(matches!(
        editor.apply_operation(replace(NameScope::Selected, "a", "x")),
        Err(OperationError::NoSelection)
    ));
}

#[test]
fn a_stale_selection_generation_is_refused() {
    let (mut editor, _) = family();
    let stale = Some(editor.selection_generation() + 1);
    assert!(matches!(
        remote(&mut editor, prefix("p_", stale)),
        Err(OperationError::StaleSelection { .. })
    ));
}
