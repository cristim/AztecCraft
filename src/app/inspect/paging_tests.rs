use crate::app::{Editor, History};
use crate::editor_operations::{
    InspectPage, Operation, OperationError, PageCursor, SceneInspection, TransformInput,
    inspection_page_lower_bound,
};
use azteccraft::{
    native_scene::{NativeScene, WorldAxis},
    scene::{Primitive, Scene},
};

fn editor_named(names: impl IntoIterator<Item = String>) -> Editor {
    let mut legacy = Scene::default();
    for name in names {
        legacy.create(Primitive::Cube, &name).unwrap();
    }
    let mut editor = Editor {
        history: History::new(NativeScene::from_legacy(legacy).unwrap()),
        ..Default::default()
    };
    editor.sync_fields();
    editor
}

fn cubes(count: usize) -> Editor {
    editor_named((0..count).map(|index| format!("cube_{index:04}")))
}

type Change = fn(&mut Editor);

fn page(limit: usize, cursor: Option<PageCursor>) -> InspectPage {
    InspectPage { limit, cursor }
}

fn cursor_of(inspection: &SceneInspection) -> PageCursor {
    inspection
        .page
        .as_ref()
        .unwrap()
        .next_cursor
        .as_ref()
        .unwrap()
        .parse()
        .unwrap()
}

fn walk(editor: &Editor, limit: usize) -> Vec<SceneInspection> {
    let mut pages = Vec::new();
    let mut cursor = None;
    loop {
        let inspection = editor
            .inspect_page(false, Some(&page(limit, cursor)))
            .unwrap();
        let more = inspection.page.as_ref().unwrap().truncated;
        if more {
            cursor = Some(cursor_of(&inspection));
        }
        pages.push(inspection);
        if !more {
            return pages;
        }
        // A cursor that never ends must fail here, not exhaust memory.
        assert!(pages.len() <= editor.history.scene().nodes().len() / limit + 1);
    }
}

fn node_ids(editor: &Editor) -> Vec<String> {
    editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|node| node.id().to_decimal())
        .collect()
}

#[test]
fn pages_concatenate_to_the_whole_scene_in_order() {
    let editor = cubes(1000);
    let pages = walk(&editor, 200);
    assert_eq!(pages.len(), 5);
    let nodes: Vec<_> = pages
        .iter()
        .flat_map(|p| p.nodes.iter().map(|n| n.id.clone()))
        .collect();
    assert_eq!(nodes, node_ids(&editor));
    let shapes: Vec<_> = pages
        .iter()
        .flat_map(|p| p.shapes.iter().map(|s| s.id.clone()))
        .collect();
    let expected: Vec<_> = editor
        .history
        .scene()
        .shapes()
        .iter()
        .map(|shape| shape.id().to_decimal())
        .collect();
    assert_eq!(shapes, expected);
    for (index, inspection) in pages.iter().enumerate() {
        let block = inspection.page.as_ref().unwrap();
        assert_eq!(
            (block.nodes_total, block.node_offset, block.node_count),
            (1000, index * 200, 200)
        );
        assert_eq!(block.truncated, index < 4);
        assert_eq!(block.next_cursor.is_some(), index < 4);
    }
}

#[test]
fn an_exact_multiple_ends_without_an_empty_extra_page() {
    let pages = walk(&cubes(6), 3);
    assert_eq!(pages.len(), 2);
    assert!(!pages[1].page.as_ref().unwrap().truncated);
}

#[test]
fn no_page_means_no_page_block_and_all_nodes() {
    let editor = cubes(5);
    let plain = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    assert!(plain.get("page").is_none());
    assert_eq!(plain["nodes"].as_array().unwrap().len(), 5);
    let paged = editor.inspect_page(false, Some(&page(2, None))).unwrap();
    assert_eq!(paged.nodes.len(), 2);
}

#[test]
fn every_scene_change_makes_a_cursor_stale_and_other_changes_do_not() {
    let identity = || TransformInput {
        translation: [0.0; 3],
        rotation_degrees: [0.0; 3],
        scale: [1.0; 3],
    };
    let begin = |editor: &mut Editor| {
        let node = editor.history.scene().nodes()[0].id();
        editor
            .apply_operation(Operation::Select {
                node: Some(node),
                instance_path: None,
            })
            .unwrap();
        let target = editor.selected_move_target().unwrap();
        editor
            .apply_operation(Operation::BeginMove {
                target,
                axis: WorldAxis::X,
            })
            .unwrap();
        editor.history.active_manipulation().unwrap().id
    };
    let update = |editor: &mut Editor, id| {
        editor
            .apply_operation(Operation::UpdateMove {
                id,
                distance: crate::editor_operations::MoveDistance::Centimeters(3.0),
            })
            .unwrap();
    };
    let stale: Vec<(&str, Change)> = vec![
        ("create", |e| {
            e.apply_operation(Operation::Create {
                primitive: Primitive::Cube,
                name: None,
                options: None,
            })
            .unwrap();
        }),
        ("delete", |e| {
            let id = e.history.scene().nodes()[0].id();
            e.apply_operation(Operation::Delete(id)).unwrap();
        }),
        ("rename", |e| {
            let id = e.history.scene().nodes()[0].id();
            e.apply_operation(Operation::Rename {
                id,
                name: "renamed".into(),
            })
            .unwrap();
        }),
        ("set_transform", |e| {
            let id = e.history.scene().nodes()[0].id();
            e.apply_operation(Operation::SetTransform {
                id,
                transform: TransformInput {
                    translation: [5.0, 0.0, 0.0],
                    rotation_degrees: [0.0; 3],
                    scale: [1.0; 3],
                },
            })
            .unwrap();
        }),
        ("new token", |e| e.scene_token = uuid::Uuid::new_v4()),
    ];
    for (label, change) in stale {
        let mut editor = cubes(6);
        let first = editor.inspect_page(false, Some(&page(2, None))).unwrap();
        change(&mut editor);
        let result = editor.inspect_page(false, Some(&page(2, Some(cursor_of(&first)))));
        assert!(matches!(result, Err(OperationError::StaleScene)), "{label}");
    }
    // Undo and redo, from a history that has one edit to step over.
    let mut editor = cubes(6);
    editor
        .apply_operation(Operation::Rename {
            id: editor.history.scene().nodes()[0].id(),
            name: "renamed".into(),
        })
        .unwrap();
    for (label, step) in [("undo", Operation::Undo), ("redo", Operation::Redo)] {
        let first = editor.inspect_page(false, Some(&page(2, None))).unwrap();
        editor.apply_operation(step).unwrap();
        let result = editor.inspect_page(false, Some(&page(2, Some(cursor_of(&first)))));
        assert!(matches!(result, Err(OperationError::StaleScene)), "{label}");
    }
    // A move in flight: the preview, then its cancel and its commit.
    for finish in [
        azteccraft::history::ManipulationFinish::Cancel,
        azteccraft::history::ManipulationFinish::Commit,
    ] {
        let mut editor = cubes(6);
        let first = editor.inspect_page(false, Some(&page(2, None))).unwrap();
        let id = begin(&mut editor);
        update(&mut editor, id);
        let result = editor.inspect_page(false, Some(&page(2, Some(cursor_of(&first)))));
        assert!(matches!(result, Err(OperationError::StaleScene)), "preview");
        let mid = editor.inspect_page(false, Some(&page(2, None))).unwrap();
        editor
            .apply_operation(Operation::FinishMove { id, finish })
            .unwrap();
        let result = editor.inspect_page(false, Some(&page(2, Some(cursor_of(&mid)))));
        assert!(
            matches!(result, Err(OperationError::StaleScene)),
            "{finish:?}"
        );
    }
    // These change no scene data.
    let mut editor = cubes(6);
    let first = editor.inspect_page(false, Some(&page(2, None))).unwrap();
    let id = editor.history.scene().nodes()[0].id();
    for operation in [
        Operation::Select {
            node: Some(id),
            instance_path: None,
        },
        Operation::Orbit {
            radians: [0.1, 0.1],
        },
        Operation::SetSelectionMode(crate::editor_operations::SelectionMode::Hierarchy),
        Operation::SetTransform {
            id,
            transform: identity(),
        },
    ] {
        editor.apply_operation(operation).unwrap();
        let next = editor
            .inspect_page(false, Some(&page(2, Some(cursor_of(&first)))))
            .unwrap();
        assert_eq!(next.nodes.len(), 2);
    }
}

#[test]
fn a_forged_cursor_is_refused_with_a_typed_error() {
    let editor = cubes(6);
    let valid = cursor_of(&editor.inspect_page(false, Some(&page(2, None))).unwrap());
    let with = |offset, revision| PageCursor {
        offset,
        revision,
        ..valid
    };
    let ask = |limit, cursor| editor.inspect_page(false, Some(&page(limit, Some(cursor))));
    let last = ask(2, with(6, valid.revision)).unwrap();
    assert!(last.nodes.is_empty() && !last.page.unwrap().truncated);
    for offset in [7, usize::MAX] {
        assert!(matches!(
            ask(2, with(offset, valid.revision)),
            Err(OperationError::InvalidInput(_))
        ));
    }
    assert!(matches!(
        ask(usize::MAX, with(4, valid.revision)).map(|p| p.nodes.len()),
        Err(OperationError::InvalidInput(_))
    ));
    assert_eq!(ask(4096, with(4, valid.revision)).unwrap().nodes.len(), 2);
    assert!(matches!(
        ask(2, with(2, valid.revision.wrapping_add(1))),
        Err(OperationError::StaleScene)
    ));
    // The offset is checked only after the token and revision match.
    assert!(matches!(
        ask(2, with(usize::MAX, valid.revision.wrapping_add(1))),
        Err(OperationError::StaleScene)
    ));
    for limit in [0, 4097] {
        assert!(matches!(
            editor.inspect_page(false, Some(&page(limit, None))),
            Err(OperationError::InvalidInput(_))
        ));
    }
}

fn three_curves() -> Editor {
    let cc = " setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 1 0 2 -1 0 3 0 0;";
    let mut source = String::from(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film;",
    );
    for index in 0..3 {
        source.push_str(&format!(
            " createNode transform -n \"t{index}\"; createNode nurbsCurve -n \"c{index}\" -p \"t{index}\";{cc}"
        ));
    }
    let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
    Editor {
        history: History::new(scene),
        ..Default::default()
    }
}

#[test]
fn each_curve_appears_on_exactly_the_page_of_its_node() {
    let pages = walk(&three_curves(), 1);
    assert_eq!(pages.len(), 3);
    for (index, inspection) in pages.iter().enumerate() {
        let names: Vec<_> = inspection.curves.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, [format!("c{index}")]);
    }
}

#[test]
fn the_selection_and_session_materials_repeat_on_every_page() {
    let mut editor = cubes(6);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths);
    let pages = walk(&editor, 4);
    assert_eq!(pages.len(), 2);
    assert!(!pages[0].session_materials.is_empty());
    for inspection in &pages {
        assert_eq!(inspection.selection.len(), 6);
        assert_eq!(inspection.selection_count, 6);
        assert_eq!(inspection.scene_token, pages[0].scene_token);
    }
    assert_eq!(
        serde_json::to_value(&pages[0].session_materials).unwrap(),
        serde_json::to_value(&pages[1].session_materials).unwrap()
    );
}

fn page_paths(
    editor: &Editor,
    range: &std::ops::Range<usize>,
) -> Vec<azteccraft::native_scene::TransformTarget> {
    let scene = editor.history.scene();
    let ids: std::collections::HashSet<_> = scene.nodes()[range.clone()]
        .iter()
        .map(|node| node.id())
        .collect();
    scene
        .transform_paths()
        .unwrap()
        .into_iter()
        .filter(|target| ids.contains(&target.node))
        .collect()
}

/// A page is charged its own nodes and its own instance paths and nothing from other pages.
#[test]
fn a_page_is_charged_exactly_its_own_bound() {
    let editor = cubes(6);
    let first = editor.inspect_page(false, Some(&page(2, None))).unwrap();
    let request = page(2, Some(cursor_of(&first)));
    let range = editor.page_range(&request).unwrap();
    let bound = inspection_page_lower_bound(editor.history.scene(), range.clone())
        + editor.path_lower_bound(&page_paths(&editor, &range));
    assert!(editor.inspect_within(false, bound, Some(&request)).is_ok());
    assert!(matches!(
        editor.inspect_within(false, bound - 1, Some(&request)),
        Err(OperationError::TooLarge)
    ));
}

/// The page bound must never exceed the serialized page, for escaped names, a full selection
/// and curves alike.
#[test]
fn the_page_bound_never_exceeds_the_serialized_page() {
    let quoted = editor_named((0..7).map(|i| format!("q\"\u{e9}\u{20ac}{i}")));
    let mut selected = cubes(30);
    let paths = selected.history.scene().transform_paths().unwrap();
    selected.set_selection_members_for_test(paths);
    for (editor, limit) in [
        (cubes(50), 7),
        (quoted, 3),
        (selected, 8),
        (three_curves(), 1),
    ] {
        let scene = editor.history.scene();
        let mut cursor = None;
        let mut seen = 0;
        loop {
            seen += 1;
            assert!(seen <= scene.nodes().len() / limit + 1, "cursor never ends");
            let request = page(limit, cursor);
            let inspection = editor.inspect_page(false, Some(&request)).unwrap();
            let range = editor.page_range(&request).unwrap();
            let paths = page_paths(&editor, &range);
            let bound = inspection_page_lower_bound(scene, range) + editor.path_lower_bound(&paths);
            let real = serde_json::to_vec(&inspection).unwrap().len();
            assert!(bound <= real, "{bound} > {real}");
            if !inspection.page.as_ref().unwrap().truncated {
                break;
            }
            cursor = Some(cursor_of(&inspection));
        }
    }
}

#[test]
fn an_oversized_page_is_refused_while_a_smaller_page_of_the_same_scene_is_not() {
    let whole = cubes(4096);
    assert!(matches!(
        whole.inspect_page(false, Some(&page(4096, None))),
        Err(OperationError::TooLarge)
    ));
    let name = "s".repeat(3500);
    let hostile = editor_named((0..40).map(|i| format!("{i:02}{name}")));
    assert!(matches!(
        hostile.inspect_page(false, Some(&page(40, None))),
        Err(OperationError::TooLarge)
    ));
    assert_eq!(
        hostile
            .inspect_page(false, Some(&page(10, None)))
            .unwrap()
            .nodes
            .len(),
        10
    );
}

/// Every page repeats the whole selection, so a full selection only lowers the usable
/// `node_limit`; it does not block inspection.
#[test]
fn a_full_selection_inspects_with_a_small_node_limit() {
    let mut editor = cubes(4096);
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths);
    let one = editor.inspect_page(false, Some(&page(1, None))).unwrap();
    assert_eq!((one.selection.len(), one.nodes.len()), (4096, 1));
    let bytes = serde_json::to_vec(&one).unwrap().len();
    assert!(bytes <= crate::editor_operations::MAX_REPLY_BYTES);
    assert!(editor.inspect_page(false, Some(&page(250, None))).is_err());
}
