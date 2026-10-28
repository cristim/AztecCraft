use super::*;
use crate::app::tests::{material_editor, remote};
use crate::editor_operations::SidebarPanel::{AttributeEditor as AE, ChannelBox as CB};
use azteccraft::history::ManipulationProvenance;
use azteccraft::native_scene::WorldAxis;

fn set(panel: SidebarPanel, visible: bool) -> Operation {
    Operation::SetSidebarPanelVisible { panel, visible }
}

fn tab_op(tab: AttributeEditorTab) -> Operation {
    Operation::SelectAttributeEditorTab { tab }
}

/// The two operations under test; the node is the selected transform.
type Make = fn(NodeId) -> Operation;
const MAKERS: [Make; 2] = [
    |_| set(AE, true),
    |node| tab_op(AttributeEditorTab::Transform(node)),
];

/// A cube with the Attribute Editor active, the only dock state that accepts a tab selection.
fn cube() -> Editor {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.sidebar_dock = Some(AE);
    editor
}

fn shape_of(editor: &Editor) -> NativeId {
    editor.history.scene().shapes()[0].id()
}

fn inspected(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.inspect(false).unwrap()).unwrap()
}

type Cell = (Option<SidebarPanel>, bool);

#[test]
fn transition_table_covers_every_cell_natively_and_remotely() {
    let ops = [(CB, true), (CB, false), (AE, true), (AE, false)];
    let table: [(Option<SidebarPanel>, [Cell; 4]); 3] = [
        (
            Some(CB),
            [
                (Some(CB), false),
                (None, true),
                (Some(AE), true),
                (Some(CB), false),
            ],
        ),
        (
            Some(AE),
            [
                (Some(CB), true),
                (Some(AE), false),
                (Some(AE), false),
                (None, true),
            ],
        ),
        (
            None,
            [
                (Some(CB), true),
                (None, false),
                (Some(AE), true),
                (None, false),
            ],
        ),
    ];
    for (current, cells) in table {
        for ((panel, visible), (expected, changed)) in ops.into_iter().zip(cells) {
            for remote_caller in [false, true] {
                let mut editor = Editor {
                    sidebar_dock: current,
                    ..Editor::default()
                };
                let reply = if remote_caller {
                    remote(&mut editor, set(panel, visible))
                } else {
                    editor.apply_operation(set(panel, visible))
                }
                .unwrap();
                let cell = format!("{current:?} set({panel:?},{visible}) remote={remote_caller}");
                assert_eq!(editor.sidebar_dock, expected, "{cell}");
                assert_eq!(matches!(reply, OperationReply::Changed), changed, "{cell}");
                assert_eq!(
                    matches!(reply, OperationReply::Unchanged),
                    !changed,
                    "{cell}"
                );
                let json = inspected(&editor);
                assert_eq!(json["sidebar_panels"]["channel_box"], expected == Some(CB));
                assert_eq!(
                    json["sidebar_panels"]["attribute_editor"],
                    expected == Some(AE)
                );
            }
        }
    }
}

#[test]
fn dock_switch_is_view_state_only_with_no_history_or_status() {
    let mut editor = cube();
    let scene = editor.history.scene().clone();
    editor.status = "sentinel".into();
    editor.native_operation(set(AE, true));
    editor.native_operation(set(CB, true));
    assert_eq!(editor.status, "sentinel");
    assert_eq!(editor.history.scene(), &scene);
    // The only undoable step is the Create; a dock switch added none.
    editor.history_step(false);
    assert!(editor.history.scene().nodes().is_empty());
    assert!(!editor.history.can_undo());
}

#[test]
fn tab_is_null_when_hidden_and_survives_a_dock_round_trip() {
    let mut editor = cube();
    editor.sidebar_dock = Some(CB);
    let shape = shape_of(&editor);
    assert!(inspected(&editor)["attribute_editor_tab"].is_null());
    editor.apply_operation(set(AE, true)).unwrap();
    let node = editor.selected().unwrap();
    assert_eq!(
        inspected(&editor)["attribute_editor_tab"],
        serde_json::json!({"kind": "transform", "id": node.to_decimal()})
    );
    editor
        .apply_operation(tab_op(AttributeEditorTab::Shape(shape)))
        .unwrap();
    assert_eq!(
        inspected(&editor)["attribute_editor_tab"],
        serde_json::json!({"kind": "shape", "id": shape.to_decimal()})
    );
    editor.apply_operation(set(CB, true)).unwrap();
    assert!(inspected(&editor)["attribute_editor_tab"].is_null());
    editor.apply_operation(set(AE, true)).unwrap();
    assert_eq!(inspected(&editor)["attribute_editor_tab"]["kind"], "shape");
    editor.select(None);
    assert!(inspected(&editor)["attribute_editor_tab"].is_null());
}

#[test]
fn tab_selection_replies_and_drives_the_material_shape() {
    let mut editor = material_editor();
    let node = editor.selected().unwrap();
    let shape = shape_of(&editor);
    editor.apply_operation(set(AE, true)).unwrap();
    assert!(matches!(
        remote(&mut editor, tab_op(AttributeEditorTab::Transform(node))),
        Ok(OperationReply::Unchanged)
    ));
    for (tab, kind) in [
        (AttributeEditorTab::Shape(shape), "shape"),
        (AttributeEditorTab::Material(shape), "material"),
    ] {
        editor.material_shape = None;
        assert!(matches!(
            remote(&mut editor, tab_op(tab)),
            Ok(OperationReply::Changed)
        ));
        assert_eq!(editor.material_shape, Some(shape));
        assert!(matches!(
            remote(&mut editor, tab_op(tab)),
            Ok(OperationReply::Unchanged)
        ));
        assert_eq!(inspected(&editor)["attribute_editor_tab"]["kind"], kind);
    }
    assert!(matches!(
        remote(&mut editor, tab_op(AttributeEditorTab::Transform(node))),
        Ok(OperationReply::Changed)
    ));
}

#[test]
fn unresolved_tabs_are_refused_with_invalid_input_and_change_nothing() {
    let mut editor = material_editor();
    editor.sidebar_dock = Some(AE);
    let node = editor.selected().unwrap();
    let shape = shape_of(&editor);
    let foreign = editor.history.scene().shapes()[1].id();
    let other_node = editor.history.scene().nodes()[1].id();
    let own_transform = editor.history.scene().nodes()[0].transform_id();
    let before = inspected(&editor);
    for tab in [
        AttributeEditorTab::Shape(foreign),
        AttributeEditorTab::Material(foreign),
        AttributeEditorTab::Transform(other_node),
        AttributeEditorTab::Shape(own_transform),
        AttributeEditorTab::Material(own_transform),
    ] {
        for remote_caller in [false, true] {
            let result = if remote_caller {
                remote(&mut editor, tab_op(tab))
            } else {
                editor.apply_operation(tab_op(tab))
            };
            assert!(
                matches!(result, Err(OperationError::InvalidInput(_))),
                "{tab:?}"
            );
            assert_eq!(editor.attribute_editor_tab, None);
            assert_eq!(inspected(&editor), before);
        }
    }
    editor.select(None);
    for tab in [
        AttributeEditorTab::Shape(shape),
        AttributeEditorTab::Transform(node),
    ] {
        assert!(matches!(
            editor.apply_operation(tab_op(tab)),
            Err(OperationError::InvalidInput(_))
        ));
    }
}

#[test]
fn tab_resets_on_selection_change_deletion_shape_removal_and_scene_replacement() {
    let mut editor = cube();
    let first = editor.selected().unwrap();
    let shape = shape_of(&editor);
    let pick = |editor: &mut Editor, shape| {
        editor
            .apply_operation(tab_op(AttributeEditorTab::Shape(shape)))
            .unwrap();
        assert_eq!(
            editor.attribute_editor_tab,
            Some(AttributeEditorTab::Shape(shape))
        );
    };
    // Re-selecting the same node keeps the tab.
    pick(&mut editor, shape);
    editor.select(Some(first));
    assert_eq!(
        editor.attribute_editor_tab,
        Some(AttributeEditorTab::Shape(shape))
    );
    // Selection change, including back to the original node.
    editor.create(Primitive::Plane);
    assert_eq!(editor.attribute_editor_tab, None);
    editor.select(Some(first));
    assert_eq!(editor.attribute_editor_tab, None);
    assert_eq!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(first))
    );
    // Deselect.
    pick(&mut editor, shape);
    editor.select(None);
    assert_eq!(editor.attribute_editor_tab, None);
    // Node deletion.
    editor.select(Some(first));
    pick(&mut editor, shape);
    editor.apply_operation(Operation::Delete(first)).unwrap();
    assert_eq!(editor.attribute_editor_tab, None);
    // Undo of the Delete restores the node: the tab stays at the transform.
    editor.history_step(false);
    editor.select(Some(first));
    assert_eq!(editor.attribute_editor_tab, None);
    // Shape removal: Undo of the Create removes the shape with its node.
    let mut editor = cube();
    let shape = shape_of(&editor);
    pick(&mut editor, shape);
    editor.history_step(false);
    assert_eq!(editor.attribute_editor_tab, None);
    assert!(editor.effective_attribute_editor_tab().is_none());
    // A stored tab that no longer resolves is dropped by sync.
    let mut editor = cube();
    let shape = shape_of(&editor);
    editor.attribute_editor_tab = Some(AttributeEditorTab::Shape(
        editor.history.scene().nodes()[0].transform_id(),
    ));
    assert_ne!(Some(shape), None);
    editor.sync_fields();
    assert_eq!(editor.attribute_editor_tab, None);
}

#[test]
fn scene_replacement_resets_the_tab() {
    let mut editor = cube();
    let shape = shape_of(&editor);
    editor
        .apply_operation(tab_op(AttributeEditorTab::Shape(shape)))
        .unwrap();
    editor.apply_operation(Operation::NewScene).unwrap();
    // The cube is unsaved, so New asks first; confirm discarding it.
    if let Some(pending) = editor.files.pending_inspection() {
        editor
            .apply_operation(Operation::ResolveFileAction {
                pending_id: pending.id.parse().unwrap(),
                decision: crate::editor_operations::FileDecision::Discard,
            })
            .unwrap();
    }
    assert!(editor.history.scene().nodes().is_empty());
    assert_eq!(editor.attribute_editor_tab, None);
}

#[test]
fn a_stored_material_tab_without_a_material_is_never_reported() {
    let mut editor = cube();
    let shape = shape_of(&editor);
    editor.attribute_editor_tab = Some(AttributeEditorTab::Material(shape));
    assert!(matches!(
        editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::Transform(_))
    ));
    editor.apply_operation(set(AE, true)).unwrap();
    assert_eq!(
        inspected(&editor)["attribute_editor_tab"]["kind"],
        "transform"
    );
}

#[test]
fn native_refuses_closing_and_modal_before_committing_drafts() {
    for closing in [false, true] {
        for make in MAKERS {
            let mut editor = Editor::default();
            if closing {
                editor.close_document().unwrap();
            } else {
                editor.create(Primitive::Cube);
                editor.new_document().unwrap();
            }
            let node = editor
                .selected()
                .unwrap_or_else(|| NodeId::from_decimal("1").unwrap());
            editor.name = "pending draft".into();
            let before = inspected(&editor);
            assert!(matches!(
                editor.apply_operation(make(node)),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(inspected(&editor), before);
            assert_eq!(editor.name, "pending draft");
        }
    }
}

#[test]
fn native_commits_a_valid_draft_and_refuses_an_invalid_one() {
    for make in MAKERS {
        let mut editor = cube();
        let dock = editor.sidebar_dock;
        let node = editor.selected().unwrap();
        let scene = editor.history.scene().clone();
        editor.channels[0] = "invalid".into();
        assert!(matches!(
            editor.apply_operation(make(node)),
            Err(OperationError::InvalidInput(_))
        ));
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.sidebar_dock, dock);
        assert_eq!(editor.channels[0], "invalid");
        editor.channels[0] = "2".into();
        editor.apply_operation(make(node)).unwrap();
        assert_eq!(
            editor.history.scene().nodes()[0].transform().translation[0],
            2.0
        );
        assert!(!editor.has_draft());
    }
}

#[test]
fn remote_refuses_any_draft_native_input_and_stale_token_with_state_intact() {
    for make in MAKERS {
        let mut editor = cube();
        let dock = editor.sidebar_dock;
        let node = editor.selected().unwrap();
        let before = inspected(&editor);
        for draft in ["2", "invalid"] {
            editor.channels[0] = draft.into();
            assert!(matches!(
                remote(&mut editor, make(node)),
                Err(OperationError::BusyEditing)
            ));
            assert_eq!(editor.channels[0], draft);
            assert_eq!(editor.sidebar_dock, dock);
        }
        editor.channels = Default::default();
        editor.sync_fields();
        editor.native_input_pending = true;
        assert!(matches!(
            remote(&mut editor, make(node)),
            Err(OperationError::BusyEditing)
        ));
        editor.native_input_pending = false;
        let stale = editor.remote_operation(
            RemoteRequest {
                expected_scene_token: Some(Uuid::new_v4()),
                operation: make(node),
            },
            &egui::Context::default(),
        );
        assert!(matches!(stale, Err(OperationError::StaleScene)));
        assert_eq!(inspected(&editor), before);
    }
}

#[test]
fn an_active_move_refuses_both_operations_for_both_callers() {
    let mut editor = cube();
    let dock = editor.sidebar_dock;
    let node = editor.selected().unwrap();
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation_guarded(
            ManipulationProvenance::Remote,
            Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            },
            &|| true,
        )
        .unwrap();
    for make in MAKERS {
        for provenance in [
            ManipulationProvenance::Remote,
            ManipulationProvenance::Native,
        ] {
            assert!(matches!(
                editor.apply_operation_guarded(provenance, make(node), &|| true),
                Err(OperationError::BusyManipulation)
            ));
        }
        assert!(matches!(
            remote(&mut editor, make(node)),
            Err(OperationError::BusyManipulation)
        ));
        editor.status.clear();
        editor.native_operation(make(node));
        assert_eq!(editor.status, OperationError::BusyManipulation.to_string());
    }
    assert_eq!(editor.sidebar_dock, dock);
    assert_eq!(editor.attribute_editor_tab, None);
}

#[test]
fn tab_selection_is_refused_unless_the_attribute_editor_is_the_active_panel() {
    for dock in [Some(CB), None] {
        let mut editor = cube();
        editor.sidebar_dock = dock;
        let node = editor.selected().unwrap();
        let before = inspected(&editor);
        for remote_caller in [false, true] {
            let operation = tab_op(AttributeEditorTab::Transform(node));
            let result = if remote_caller {
                remote(&mut editor, operation)
            } else {
                editor.apply_operation(operation)
            };
            let Err(OperationError::InvalidInput(message)) = result else {
                panic!("hidden Attribute Editor must refuse a tab");
            };
            assert!(message.contains("Attribute Editor"), "{message}");
            assert_eq!(editor.attribute_editor_tab, None);
            assert_eq!(editor.sidebar_dock, dock);
            assert_eq!(inspected(&editor), before);
        }
    }
}
