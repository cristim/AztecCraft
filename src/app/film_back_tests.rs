//! Issue #458 T2: `SetCameraFilmBack` through the real Editor dispatch. The guard rows (held
//! drag, draft, modal, close, stale token) are in `guard_matrix_tests.rs`.
use super::*;
use azteccraft::viewport::{FilmBack, FilmBackEdit, FilmFit, Lens};

/// 34.999999999999993: an f64 that an f32 or rounded text would lose.
const ONE_ULP_BELOW_35: f64 = f64::from_bits(35.0_f64.to_bits() - 1);

fn film_edit(edit: impl FnOnce(&mut FilmBackEdit)) -> Operation {
    let mut film = FilmBackEdit::default();
    edit(&mut film);
    Operation::SetCameraFilmBack(film)
}

fn remote_film(
    editor: &mut Editor,
    operation: Operation,
) -> Result<OperationReply, OperationError> {
    let token = editor.scene_token;
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation,
        },
        &egui::Context::default(),
    )
}

fn film(editor: &Editor) -> FilmBack {
    match editor.camera.lens() {
        Lens::Film(film) => film,
        Lens::VerticalFov(_) => panic!("expected a film lens"),
    }
}

fn fov_lens_editor() -> Editor {
    Editor {
        camera: Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 1.0, 0.1, 100.0).unwrap(),
        ..Editor::default()
    }
}

#[test]
fn edit_changes_only_the_named_fields_and_leaves_scene_and_history_alone() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    let (dirty, scene, undoable) = (
        editor.is_dirty(),
        editor.history.scene().clone(),
        editor.history.can_undo(),
    );
    let reply = remote_film(
        &mut editor,
        film_edit(|e| {
            e.focal_mm = Some(ONE_ULP_BELOW_35);
            e.film_roll_degrees = Some(15.0);
        }),
    )
    .unwrap();
    assert!(matches!(reply, OperationReply::Changed));
    let after = film(&editor);
    assert_eq!(after.focal_mm, ONE_ULP_BELOW_35);
    assert_eq!(after.film_roll_degrees, 15.0);
    assert_eq!(
        FilmBack {
            focal_mm: FilmBack::DEFAULT.focal_mm,
            film_roll_degrees: 0.0,
            ..after
        },
        FilmBack::DEFAULT
    );
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.can_undo(), undoable);
    let again = remote_film(&mut editor, film_edit(|e| e.film_roll_degrees = Some(15.0)));
    assert!(matches!(again.unwrap(), OperationReply::Unchanged));
}

#[test]
fn one_bad_field_leaves_every_field_and_names_itself() {
    let mut editor = Editor::default();
    let camera = editor.camera.clone();
    let error = remote_film(
        &mut editor,
        film_edit(|e| {
            e.focal_mm = Some(50.0);
            e.overscan = Some(f64::NAN);
        }),
    )
    .unwrap_err();
    assert!(matches!(error, OperationError::FilmBack(_)), "{error}");
    assert!(error.to_string().contains("overscan"), "{error}");
    assert_eq!(editor.camera, camera);
    assert!(camera.same_capture(&editor.camera));
}

#[test]
fn an_empty_edit_is_refused_and_does_not_convert_a_fov_lens() {
    let mut editor = fov_lens_editor();
    let camera = editor.camera.clone();
    let error = remote_film(&mut editor, film_edit(|_| {})).unwrap_err();
    assert!(matches!(error, OperationError::FilmBack(_)), "{error}");
    assert_eq!(editor.camera, camera);
}

#[test]
fn negative_zero_over_zero_is_unchanged_and_the_stored_bits_stay() {
    let mut editor = Editor::default();
    let reply = remote_film(&mut editor, film_edit(|e| e.film_offset_in[0] = Some(-0.0)));
    assert!(matches!(reply.unwrap(), OperationReply::Unchanged));
    assert_eq!(film(&editor).film_offset_in[0].to_bits(), 0.0_f64.to_bits());
}

#[test]
fn a_vertical_fov_lens_becomes_a_film_back_with_defaults_for_unnamed_fields() {
    let mut editor = fov_lens_editor();
    let reply = remote_film(&mut editor, film_edit(|e| e.focal_mm = Some(50.0)));
    assert!(matches!(reply.unwrap(), OperationReply::Changed));
    assert_eq!(
        film(&editor),
        FilmBack {
            focal_mm: 50.0,
            ..FilmBack::DEFAULT
        }
    );
}

#[test]
fn display_film_gate_is_a_plain_attribute_and_leaves_overscan_alone() {
    let mut editor = Editor::default();
    let gate = |on| film_edit(|e| e.display_film_gate = Some(on));
    remote_film(&mut editor, film_edit(|e| e.overscan = Some(1.2))).unwrap();
    assert!(matches!(
        remote_film(&mut editor, gate(true)).unwrap(),
        OperationReply::Changed
    ));
    assert!(film(&editor).display_film_gate);
    assert_eq!(film(&editor).overscan, 1.2, "on leaves overscan");
    assert!(matches!(
        remote_film(&mut editor, gate(true)).unwrap(),
        OperationReply::Unchanged
    ));
    remote_film(&mut editor, gate(false)).unwrap();
    assert!(!film(&editor).display_film_gate);
    assert_eq!(film(&editor).overscan, 1.2, "off leaves overscan");
}

#[test]
fn editing_in_orthographic_is_allowed_and_does_not_change_the_projection() {
    let mut editor = Editor::default();
    editor
        .camera
        .show_orthographic(OrthographicView::Top)
        .unwrap();
    let matrix = editor.camera.projection_matrix(1.5).unwrap();
    let reply = remote_film(
        &mut editor,
        film_edit(|e| e.film_fit = Some(FilmFit::Vertical)),
    );
    assert!(matches!(reply.unwrap(), OperationReply::Changed));
    assert_eq!(editor.camera.projection(), Projection::Orthographic);
    assert_eq!(editor.camera.projection_matrix(1.5).unwrap(), matrix);
    assert_eq!(film(&editor).film_fit, FilmFit::Vertical);
}

#[test]
fn a_stale_token_is_refused_and_native_dispatch_matches_remote() {
    let mut editor = Editor::default();
    let camera = editor.camera.clone();
    let error = editor
        .remote_operation(
            RemoteRequest {
                expected_scene_token: Some(Uuid::new_v4()),
                operation: film_edit(|e| e.focal_mm = Some(50.0)),
            },
            &egui::Context::default(),
        )
        .unwrap_err();
    assert!(matches!(error, OperationError::StaleScene));
    assert_eq!(editor.camera, camera);
    editor.native_operation(film_edit(|e| e.focal_mm = Some(50.0)));
    assert_eq!(film(&editor).focal_mm, 50.0);
    assert!(!editor.error);
    editor.native_operation(film_edit(|e| e.focal_mm = Some(-1.0)));
    assert_eq!(film(&editor).focal_mm, 50.0);
    assert!(editor.error);
}
