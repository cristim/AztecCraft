use super::*;
use crate::app::tests::deep_group_editor;
use azteccraft::native_scene::{NativeScene, NativeTransform};
use azteccraft::scene::Primitive;
use glam::Vec2;

const WHOLE: [[f32; 2]; 2] = [[-1.0, -1.0], [1.0, 1.0]];

fn rect(bounds: [[f32; 2]; 2]) -> NdcRect {
    NdcRect::new(Vec2::from(bounds[0]), Vec2::from(bounds[1])).unwrap()
}

fn marquee(
    editor: &mut Editor,
    bounds: [[f32; 2]; 2],
    mode: MarqueeMode,
) -> Result<OperationReply, OperationError> {
    editor.apply_operation(Operation::SelectInRect {
        rect: rect(bounds),
        mode,
    })
}

fn changed(reply: Result<OperationReply, OperationError>) -> bool {
    matches!(reply.unwrap(), OperationReply::Changed)
}

/// Cubes named `c0`.. at the given x positions and scales, nothing selected.
fn editor_with(cubes: &[(f64, f64)]) -> Editor {
    let mut scene = NativeScene::default();
    for (index, (x, scale)) in cubes.iter().enumerate() {
        let node = scene.create(Primitive::Cube, &format!("c{index}")).unwrap();
        scene
            .set_transform(
                node,
                NativeTransform {
                    translation: [*x, 0.0, 0.0],
                    scale: [*scale; 3],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    editor_of(scene)
}

fn editor_of(scene: NativeScene) -> Editor {
    let mut editor = Editor {
        history: History::new(scene),
        ..Editor::default()
    };
    editor.sync_fields();
    editor
}

fn cubes_at(xs: &[f64]) -> Editor {
    editor_with(&xs.iter().map(|x| (*x, 1.0)).collect::<Vec<_>>())
}

fn paths(editor: &Editor) -> Vec<TransformTarget> {
    editor.history.scene().transform_paths().unwrap()
}

/// A small square rect around the projection of the `index`-th cube's centre.
fn around(editor: &Editor, index: usize) -> [[f32; 2]; 2] {
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let vertices = view.instances()[index].world_vertices();
    let centre = vertices.iter().copied().sum::<glam::Vec3>() / vertices.len() as f32;
    let ndc = editor
        .camera
        .project(centre, editor.aspect)
        .unwrap()
        .unwrap();
    [[ndc.x - 0.01, ndc.y - 0.01], [ndc.x + 0.01, ndc.y + 0.01]]
}

fn select_only(editor: &mut Editor, target: &TransformTarget) {
    editor
        .apply_operation(Operation::Select {
            node: Some(target.node),
            instance_path: Some(target.path.clone()),
        })
        .unwrap();
}

#[test]
fn marquee_mode_maps_the_modifiers() {
    let mods = |shift, ctrl, command| egui::Modifiers {
        shift,
        ctrl,
        command,
        ..egui::Modifiers::NONE
    };
    assert_eq!(
        marquee_mode(mods(false, false, false)),
        MarqueeMode::Replace
    );
    assert_eq!(marquee_mode(mods(true, false, false)), MarqueeMode::Toggle);
    assert_eq!(marquee_mode(mods(false, true, false)), MarqueeMode::Remove);
    assert_eq!(marquee_mode(mods(false, false, true)), MarqueeMode::Remove);
    assert_eq!(marquee_mode(mods(true, true, false)), MarqueeMode::Add);
    assert_eq!(marquee_mode(mods(true, false, true)), MarqueeMode::Add);
}

#[test]
fn replace_selects_the_hits_in_instance_order_with_the_last_as_lead() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    let all = paths(&editor);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert_eq!(editor.selection.members(), all);
    assert_eq!(editor.selection.lead(), all.last());
}

#[test]
fn marquee_lead_is_the_last_hit_in_instance_order() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[2]);
    // Only the first two cubes are in the box, so the lead moves off the old third one.
    let bounds = [[-1.0, -1.0], [0.01, 1.0]];
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let hits = editor
        .camera
        .pick_rect(&view, rect(bounds), editor.aspect)
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert!(changed(marquee(&mut editor, bounds, MarqueeMode::Replace)));
    assert_eq!(editor.selection.members(), &all[..2]);
    assert_eq!(editor.selection.lead(), Some(&all[1]));
}

#[test]
fn replace_with_identical_members_keeps_generation() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    let generation = editor.selection.generation();
    assert!(!changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert_eq!(editor.selection.generation(), generation);
}

#[test]
fn add_appends_only_absent_hits_and_leaves_the_lead_when_nothing_is_new() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[1]);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Add)));
    assert_eq!(
        editor.selection.members(),
        [all[1].clone(), all[0].clone(), all[2].clone()]
    );
    assert_eq!(editor.selection.lead(), Some(&all[2]));
    let generation = editor.selection.generation();
    assert!(!changed(marquee(&mut editor, WHOLE, MarqueeMode::Add)));
    assert_eq!(editor.selection.generation(), generation);
}

#[test]
fn remove_drops_the_hit_members_and_promotes_the_previous_lead() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    let all = paths(&editor);
    marquee(&mut editor, WHOLE, MarqueeMode::Replace).unwrap();
    let bounds = around(&editor, 2);
    assert!(changed(marquee(&mut editor, bounds, MarqueeMode::Remove)));
    assert_eq!(editor.selection.members(), &all[..2]);
    assert_eq!(editor.selection.lead(), Some(&all[1]));
    let bounds = around(&editor, 2);
    assert!(!changed(marquee(&mut editor, bounds, MarqueeMode::Remove)));
}

#[test]
fn toggle_flips_members_and_appends_absent_hits() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[0]);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Toggle)));
    assert_eq!(editor.selection.members(), &all[1..]);
    assert_eq!(editor.selection.lead(), Some(&all[2]));
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Toggle)));
    assert_eq!(editor.selection.members(), [all[0].clone()]);
}

#[test]
fn an_empty_plain_marquee_clears_and_an_empty_modified_one_changes_nothing() {
    let mut editor = cubes_at(&[0.0]);
    let all = paths(&editor);
    let empty = [[0.9, 0.9], [1.0, 1.0]];
    select_only(&mut editor, &all[0]);
    for mode in [MarqueeMode::Add, MarqueeMode::Remove, MarqueeMode::Toggle] {
        let generation = editor.selection.generation();
        assert!(!changed(marquee(&mut editor, empty, mode)), "{mode:?}");
        assert_eq!(editor.selection.members(), [all[0].clone()]);
        assert_eq!(editor.selection.generation(), generation);
    }
    assert!(changed(marquee(&mut editor, empty, MarqueeMode::Replace)));
    assert!(editor.selection.is_empty());
    assert!(!changed(marquee(&mut editor, empty, MarqueeMode::Replace)));
}

/// A unit cube with a larger cube in front of it on the line of sight through the origin.
/// Returns the editor, the index of the hidden cube among the view instances, and both targets.
fn occluded_pair() -> (Editor, usize, TransformTarget, TransformTarget) {
    let mut editor = editor_with(&[(0.0, 1.0)]);
    let back = editor.camera.eye().normalize();
    let mut scene = editor.history.scene().clone();
    let front = scene.create(Primitive::Cube, "front").unwrap();
    scene
        .set_transform(
            front,
            NativeTransform {
                translation: [
                    back.x as f64 * 3.0,
                    back.y as f64 * 3.0,
                    back.z as f64 * 3.0,
                ],
                scale: [4.0; 3],
                ..Default::default()
            },
        )
        .unwrap();
    editor.history = History::new(scene);
    editor.sync_fields();
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let hidden_node = editor.history.scene().nodes()[0].id();
    let hidden = view
        .instances()
        .iter()
        .position(|instance| instance.transform() == hidden_node)
        .unwrap();
    let hit = editor
        .camera
        .pick_view(&view, Vec2::ZERO, editor.aspect)
        .unwrap()
        .unwrap();
    assert_ne!(hit.transform, hidden_node, "the front cube occludes");
    let targets = paths(&editor);
    let front = targets
        .iter()
        .find(|t| t.node == hit.transform)
        .unwrap()
        .clone();
    let back = targets
        .iter()
        .find(|t| t.node == hidden_node)
        .unwrap()
        .clone();
    (editor, hidden, front, back)
}

/// Pins the PROVISIONAL object-marquee rule: a cube wholly hidden behind a larger one is still
/// selected by an area marquee, in every shading mode. Maya documents camera-based selection
/// (occlusion filtering) for components only, so this is not a parity gap for objects; the
/// component marquee slice adds the filter for components.
#[test]
fn marquee_selects_an_occluded_object_in_every_shading_mode() {
    let (mut editor, hidden, _, back) = occluded_pair();
    let bounds = around(&editor, hidden);
    assert!(changed(marquee(&mut editor, bounds, MarqueeMode::Replace)));
    assert!(editor.selection.contains(&back));
}

/// A zero-area marquee (a long press, P11) behaves like a click: only the nearest hit.
#[test]
fn a_point_marquee_selects_only_the_nearest_object_like_a_click() {
    let (mut editor, _, front, back) = occluded_pair();
    let point = [[0.0, 0.0], [0.0, 0.0]];
    assert!(changed(marquee(&mut editor, point, MarqueeMode::Replace)));
    assert_eq!(editor.selection.members(), std::slice::from_ref(&front));
    assert!(changed(marquee(&mut editor, point, MarqueeMode::Toggle)));
    assert!(editor.selection.is_empty());
    assert!(changed(marquee(&mut editor, point, MarqueeMode::Add)));
    assert_eq!(editor.selection.members(), std::slice::from_ref(&front));
    assert!(changed(marquee(&mut editor, point, MarqueeMode::Remove)));
    assert!(!editor.selection.contains(&back));
    // A line (zero width, some height) is an area: it still reaches the hidden cube.
    let line = [[0.0, -0.01], [0.0, 0.01]];
    marquee(&mut editor, line, MarqueeMode::Replace).unwrap();
    assert!(editor.selection.contains(&back) && editor.selection.contains(&front));
}

#[test]
fn a_point_marquee_over_nothing_clears_like_an_empty_click() {
    let mut editor = cubes_at(&[0.0]);
    let cube = paths(&editor).remove(0);
    select_only(&mut editor, &cube);
    let corner = [1.0, 1.0];
    assert!(changed(marquee(
        &mut editor,
        [corner, corner],
        MarqueeMode::Replace
    )));
    assert!(editor.selection.is_empty());
}

#[test]
fn hierarchy_mode_maps_hits_to_their_root_once() {
    let mut editor = deep_group_editor();
    let all = paths(&editor);
    editor.native_operation(Operation::SetSelectionMode(SelectionMode::Hierarchy));
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert_eq!(editor.selection.members(), [all[0].clone()]);

    let mut editor = deep_group_editor();
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert_eq!(editor.selection.members(), [all[2].clone()]);
}

#[test]
fn component_modes_refuse_without_any_state_change() {
    let mut editor = cubes_at(&[0.0, 3.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[0]);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    let before = (
        editor.selection.clone(),
        editor.selection.generation(),
        editor.remembered_face.clone(),
        editor.selection_mode,
        editor.status.clone(),
    );
    for mode in [
        MarqueeMode::Replace,
        MarqueeMode::Add,
        MarqueeMode::Remove,
        MarqueeMode::Toggle,
    ] {
        assert!(matches!(
            marquee(&mut editor, WHOLE, mode),
            Err(OperationError::InvalidInput(_))
        ));
    }
    assert_eq!(
        before,
        (
            editor.selection.clone(),
            editor.selection.generation(),
            editor.remembered_face.clone(),
            editor.selection_mode,
            editor.status.clone(),
        )
    );
}

#[test]
fn a_changing_marquee_clears_remembered_components_and_a_no_op_keeps_them() {
    let mut editor = cubes_at(&[0.0, 3.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[0]);
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Face))
        .unwrap();
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    let face = FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 0,
    };
    editor
        .apply_operation(Operation::SelectFace { target: Some(face) })
        .unwrap();
    assert!(editor.remembered_face.is_some());
    editor
        .apply_operation(Operation::SetSelectionMode(SelectionMode::Object))
        .unwrap();
    assert!(editor.remembered_face.is_some(), "mode switch keeps picks");
    let empty = [[0.9, 0.9], [1.0, 1.0]];
    assert!(!changed(marquee(&mut editor, empty, MarqueeMode::Toggle)));
    assert!(editor.remembered_face.is_some());
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert!(editor.remembered_face.is_none());
}

/// `sync_fields` already drops a pick whose owner is not the lead, so only a change that keeps
/// the lead tells the explicit clear apart.
#[test]
fn a_marquee_that_keeps_the_lead_still_clears_remembered_components() {
    let mut editor = cubes_at(&[-2.0, 2.0]);
    let all = paths(&editor);
    select_only(&mut editor, &all[0]);
    let view = ViewScene::from_native(editor.history.scene(), 1 << 28).unwrap();
    let instance = &view.instances()[0];
    editor.remembered_face = Some(FaceTarget {
        transform: instance.transform(),
        shape: instance.shape(),
        path: instance.path().to_vec(),
        face_index: 0,
    });
    editor.set_selection_members_for_test(vec![all[1].clone(), all[0].clone()]);
    let bounds = around(&editor, 1);
    assert!(changed(marquee(&mut editor, bounds, MarqueeMode::Toggle)));
    assert_eq!(editor.selection.members(), [all[0].clone()]);
    assert!(editor.remembered_face.is_none());
}

#[test]
fn a_modal_dialog_refuses_and_keeps_the_selection() {
    let mut editor = Editor::default();
    editor.create(Primitive::Cube);
    editor.new_document().unwrap();
    assert!(editor.files.is_modal());
    let before = editor.selection.clone();
    assert!(matches!(
        marquee(&mut editor, WHOLE, MarqueeMode::Replace),
        Err(OperationError::BusyEditing)
    ));
    assert_eq!(editor.selection, before);
}

#[test]
fn a_marquee_over_a_curve_only_scene_selects_nothing() {
    let scene = azteccraft::native_scene::ascii::decode(std::sync::Arc::from(
        b"//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"t\"; createNode nurbsCurve -n \"s\" -p \"t\"; setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;"
            .to_vec(),
    ))
    .unwrap();
    let mut editor = editor_of(scene);
    let target = paths(&editor).remove(0);
    select_only(&mut editor, &target);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert!(editor.selection.is_empty());
}

#[test]
fn move_after_marquee_uses_the_last_hit_as_pivot() {
    let mut editor = cubes_at(&[-2.0, 2.0]);
    let all = paths(&editor);
    marquee(&mut editor, WHOLE, MarqueeMode::Replace).unwrap();
    let lead = editor.selected_move_lead().unwrap();
    assert_eq!(lead, all[1]);
    let origin = editor.move_handle_origin(&lead).unwrap();
    let frame_origin = |target: &TransformTarget| {
        editor
            .history
            .scene()
            .world_move_frame(target)
            .unwrap()
            .origin_cm()
    };
    assert_eq!(origin, frame_origin(&all[1]));
    assert_ne!(origin, frame_origin(&all[0]));
}

/// Replace over the cap needs more than 4096 hits and is pinned by `replace_with_over_the_cap_*`
/// on the selection type; Add and Toggle are driven here from a full selection of absent nodes.
#[test]
fn over_the_cap_refuses_and_changes_nothing() {
    use azteccraft::native_scene::{MAX_TRANSFORM_PATHS, NativeId};
    use azteccraft::scene::NodeId;
    let mut editor = cubes_at(&[0.0]);
    let fillers = (0..MAX_TRANSFORM_PATHS as u64)
        .map(|index| TransformTarget {
            node: NodeId::from_decimal(&(1_000_000 + index).to_string()).unwrap(),
            path: vec![NativeId::from_decimal(&(1_000_000 + index).to_string()).unwrap()],
        })
        .collect();
    editor.set_selection_members_for_test(fillers);
    let before = editor.selection.clone();
    let generation = editor.selection.generation();
    for mode in [MarqueeMode::Add, MarqueeMode::Toggle] {
        assert!(
            matches!(
                marquee(&mut editor, WHOLE, mode),
                Err(OperationError::InvalidInput(_))
            ),
            "{mode:?}"
        );
        assert_eq!(editor.selection, before);
        assert_eq!(editor.selection.generation(), generation);
    }
    // Remove needs no room, so a full selection of absent nodes just stays as it is.
    assert!(!changed(marquee(&mut editor, WHOLE, MarqueeMode::Remove)));
}
