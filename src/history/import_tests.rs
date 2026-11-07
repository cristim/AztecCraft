use super::*;
use crate::{obj::import::parse, view_scene::ViewError};

fn plan(scene: &NativeScene, text: &str) -> Command {
    let parsed = parse(text.as_bytes()).unwrap();
    Command::ImportObjects(Box::new(scene.plan_obj_import(&parsed).unwrap()))
}

const TRIANGLE: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\no t\nf 1 2 3\n";
const QUAD: &str = "v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\no q\nf 1 2 3 4\n";
const HEXAGON: &str =
    "v 2 0 0\nv 1 2 0\nv -1 2 0\nv -2 0 0\nv -1 -2 0\nv 1 -2 0\no h\nf 1 2 3 4 5 6\n";
const CONCAVE_PENTAGON: &str = "v 0 0 0\nv 4 0 0\nv 4 4 0\nv 2 1 0\nv 0 4 0\no bad\nf 1 2 3 4 5\n";

#[test]
fn an_import_is_one_undo_step_named_import_and_undo_redo_restore_the_scene() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "pCube1").unwrap();
    let before = scene.clone();
    let mut history = History::new(scene);
    let command = plan(
        history.scene(),
        "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 1 1 0\no a\nf 1 2 3\no b\nf 2 4 3\n",
    );
    let revision = history.revision();
    assert_eq!(history.execute(command).unwrap(), EditOutcome::Changed);
    assert_eq!(history.scene().nodes().len(), 3);
    assert_eq!(history.undo_label().as_deref(), Some("Import"));
    assert_ne!(history.revision(), revision);
    let imported = history.scene().clone();
    assert!(history.undo());
    assert_eq!(history.scene(), &before);
    assert!(!history.can_undo());
    assert!(history.redo());
    assert_eq!(history.scene(), &imported);
    assert!(!history.can_redo());
}

#[test]
fn triangles_quads_and_convex_hexagons_pass_view_admission() {
    for text in [TRIANGLE, QUAD, HEXAGON] {
        let mut history = History::new(NativeScene::default());
        let command = plan(history.scene(), text);
        history.execute(command).unwrap();
        assert_eq!(history.scene().nodes().len(), 1);
    }
}

#[test]
fn a_concave_pentagon_is_refused_and_scene_history_and_redo_are_untouched() {
    let mut history = History::new(NativeScene::default());
    let command = plan(history.scene(), TRIANGLE);
    history.execute(command).unwrap();
    assert!(history.undo());
    let before = history.scene().clone();
    let revision = history.revision();
    let command = plan(history.scene(), CONCAVE_PENTAGON);
    assert!(matches!(
        history.execute(command),
        Err(NativeError::UnsupportedView(text)) if text.contains("convex")
    ));
    assert_eq!(history.scene(), &before);
    assert_eq!(history.revision(), revision);
    assert!(!history.can_undo());
    assert!(history.can_redo());
}

#[test]
fn view_errors_map_to_native_errors() {
    assert_eq!(
        view_refusal(ViewError::ResourceLimit),
        NativeError::ResourceLimit
    );
    assert_eq!(
        view_refusal(ViewError::Native(NativeError::InvalidTopology)),
        NativeError::InvalidTopology
    );
    let text = |error| match view_refusal(error) {
        NativeError::UnsupportedView(text) => text,
        other => panic!("{other:?}"),
    };
    let polygon = text(ViewError::UnsupportedPolygon);
    assert!(polygon.contains("convex") && polygon.contains("triangulate"));
    let others = [
        text(ViewError::InvalidGeometry),
        text(ViewError::UnsupportedTransform),
        text(ViewError::NonFiniteWorld),
        text(ViewError::Camera(crate::viewport::InvalidCameraInput)),
    ];
    for (index, other) in others.iter().enumerate() {
        assert!(
            !other.contains("convex") && !other.contains("triangulate"),
            "{other}"
        );
        assert_ne!(*other, polygon);
        assert!(others.iter().skip(index + 1).all(|later| later != other));
    }
}
