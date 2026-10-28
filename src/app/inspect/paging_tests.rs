use crate::app::{Editor, History};
use crate::editor_operations::{InspectPage, OperationError, PageCursor, SceneInspection};
use azteccraft::{
    native_scene::NativeScene,
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
