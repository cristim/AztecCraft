use super::*;
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
fn replace_with_identical_members_keeps_generation() {
    let mut editor = cubes_at(&[-2.0, 0.0, 2.0]);
    assert!(changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    let generation = editor.selection.generation();
    assert!(!changed(marquee(&mut editor, WHOLE, MarqueeMode::Replace)));
    assert_eq!(editor.selection.generation(), generation);
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
