//! Issue #48 slice 1: the `SetView` operation shares one typed path between the native View
//! menu and MCP, with the same guards as the other session-only viewport settings.
use super::tests::remote;
use super::*;

fn view_of(editor: &Editor) -> serde_json::Value {
    serde_json::to_value(editor.inspect(false).unwrap()).unwrap()["camera"].clone()
}

#[test]
fn views_switch_idempotently_and_inspect_reports_them() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let scene = editor.history.scene().clone();
    let token = editor.scene_token;
    let dirty = editor.is_dirty();
    assert_eq!(view_of(&editor)["view"], "persp");
    assert_eq!(view_of(&editor)["projection"], "perspective");
    for (preset, name, changed) in [
        (ViewPreset::Persp, "persp", false),
        (ViewPreset::Top, "top", true),
        (ViewPreset::Top, "top", false),
        (ViewPreset::Front, "front", true),
        (ViewPreset::Side, "side", true),
        (ViewPreset::Persp, "persp", true),
        (ViewPreset::Persp, "persp", false),
    ] {
        let reply = remote(&mut editor, Operation::SetView(preset)).unwrap();
        assert_eq!(matches!(reply, OperationReply::Changed), changed, "{name}");
        assert_eq!(view_of(&editor)["view"], name);
        assert_eq!(editor.view_name(), name);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
    }
}

#[test]
fn orthographic_views_refuse_orbit_and_leave_the_camera_unchanged() {
    let mut editor = Editor::default();
    remote(&mut editor, Operation::SetView(ViewPreset::Front)).unwrap();
    let before = editor.camera.clone();
    let error = remote(
        &mut editor,
        Operation::Orbit {
            radians: [0.1, 0.1],
        },
    )
    .unwrap_err();
    assert!(
        matches!(error, OperationError::InvalidInput(_)),
        "{error:?}"
    );
    assert_eq!(editor.camera, before);
    remote(&mut editor, Operation::Dolly { factor: 2.0 }).unwrap();
    remote(&mut editor, Operation::SetView(ViewPreset::Persp)).unwrap();
    remote(
        &mut editor,
        Operation::Orbit {
            radians: [0.1, 0.1],
        },
    )
    .unwrap();
}

#[test]
fn set_view_is_refused_while_modal_or_closing_and_stale_tokens_are_stale() {
    for closing in [false, true] {
        let mut editor = Editor::default();
        if closing {
            editor.close_document().unwrap();
        } else {
            editor.create(Primitive::Cube);
            editor.new_document().unwrap();
        }
        assert!(matches!(
            editor.apply_operation(Operation::SetView(ViewPreset::Top)),
            Err(OperationError::BusyEditing)
        ));
        assert_eq!(editor.view_name(), "persp");
    }
    let mut editor = Editor::default();
    let stale = editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(Uuid::new_v4()),
            operation: Operation::SetView(ViewPreset::Top),
        },
        &egui::Context::default(),
    );
    assert!(matches!(stale, Err(OperationError::StaleScene)));
    assert_eq!(editor.view_name(), "persp");
}

#[test]
fn default_view_returns_an_orthographic_camera_to_the_default_perspective() {
    let mut editor = Editor::default();
    remote(&mut editor, Operation::SetView(ViewPreset::Side)).unwrap();
    editor.default_view();
    assert_eq!(editor.camera, Camera::default());
}

#[test]
fn a_new_scene_resets_the_camera_to_perspective_like_any_camera_state() {
    let mut editor = Editor::default();
    remote(&mut editor, Operation::SetView(ViewPreset::Side)).unwrap();
    remote(&mut editor, Operation::NewScene).unwrap();
    assert_eq!(editor.view_name(), "persp");
}

#[test]
fn a_refused_default_view_leaves_an_orthographic_camera_untouched() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    remote(&mut editor, Operation::SetView(ViewPreset::Front)).unwrap();
    editor.new_document().unwrap();
    let before = editor.camera.clone();
    editor.default_view();
    assert_eq!(editor.camera, before);
}
