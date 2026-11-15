//! Edits that would leave world geometry outside f32 range are refused, because the view builds
//! its vertices in f32 and one such vertex fails every view build until Undo (#396).
use super::*;
use crate::native_scene::{Channel, DEFAULT_JOINT_RADIUS, JointRequest, test_fixtures};
use crate::scene::Primitive;

fn channel(channel: Channel, value: f64) -> [Option<f64>; 9] {
    let mut channels = [None; 9];
    channels[channel as usize] = Some(value);
    channels
}

fn edit(
    history: &mut History,
    id: NodeId,
    channel_edit: [Option<f64>; 9],
) -> Result<(), NativeError> {
    history
        .execute(Command::EditNode {
            id,
            name: None,
            channels: channel_edit,
        })
        .map(|_| ())
}

fn refused_and_untouched(history: &mut History, command: Command) {
    let before = history.scene().clone();
    let depth = history.undo.len();
    assert_eq!(history.execute(command), Err(NativeError::WorldOutOfRange));
    assert_eq!(history.scene(), &before);
    assert_eq!(history.undo.len(), depth);
}

// The vertices of a cube at x = 3.3e38 are finite in f32, but a 1e39 scale puts the far ones
// past f32::MAX while the matrix stays finite.
#[test]
fn a_scale_edit_that_pushes_a_vertex_past_f32_is_refused() {
    let (scene, node) = test_fixtures::cube();
    let mut history = History::new(scene);
    edit(&mut history, node, channel(Channel::TranslateX, 3.3e38)).unwrap();
    for axis in [Channel::ScaleX, Channel::ScaleY, Channel::ScaleZ] {
        refused_and_untouched(
            &mut history,
            Command::EditNode {
                id: node,
                name: None,
                channels: channel(axis, 1.0e39),
            },
        );
    }
    refused_and_untouched(
        &mut history,
        Command::EditNodes(vec![(node, channel(Channel::ScaleX, 1.0e39))]),
    );
    refused_and_untouched(
        &mut history,
        Command::SetTransform {
            id: node,
            transform: NativeTransform {
                translation: [3.3e38, 0.0, 0.0],
                scale: [1.0e39, 1.0, 1.0],
                ..NativeTransform::default()
            },
        },
    );
    edit(&mut history, node, channel(Channel::ScaleX, 2.0)).unwrap();
}

#[test]
fn a_translation_that_pushes_a_vertex_past_f32_is_refused() {
    let (scene, node) = test_fixtures::cube();
    let mut history = History::new(scene);
    refused_and_untouched(
        &mut history,
        Command::EditNode {
            id: node,
            name: None,
            channels: channel(Channel::TranslateX, f64::MAX),
        },
    );
}

#[test]
fn geometry_the_view_still_draws_is_not_refused() {
    let mut scene = NativeScene::default();
    let joint = scene
        .create_joint(&JointRequest {
            parent: None,
            position: [0.0; 3],
            relative: false,
            radius: DEFAULT_JOINT_RADIUS,
            name: None,
        })
        .unwrap()
        .node;
    let mut history = History::new(scene);
    // The joint draws at its origin, so a scale whose matrix is not finite as f32 is harmless.
    edit(&mut history, joint, channel(Channel::ScaleX, 1.0e300)).unwrap();
}

#[test]
fn a_scene_already_undrawable_does_not_block_the_edit_that_repairs_it() {
    let (mut scene, node) = test_fixtures::cube();
    scene.set_channel(node, Channel::ScaleX, 1.0e300).unwrap();
    let mut history = History::new(scene);
    edit(&mut history, node, channel(Channel::ScaleX, 1.0)).unwrap();
    assert!(history.undo());
    assert!(history.redo());
}

#[test]
fn parenting_under_a_huge_scale_and_pivot_edits_are_refused_too() {
    let (mut scene, parent) = test_fixtures::cube();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    scene
        .set_channel(child, Channel::TranslateX, 1.0e10)
        .unwrap();
    scene.set_channel(parent, Channel::ScaleX, 1.0e30).unwrap();
    let mut history = History::new(scene);
    refused_and_untouched(
        &mut history,
        Command::Reparent {
            id: child,
            parent: Some(parent),
            keep: ReparentKeep::Local,
        },
    );
    refused_and_untouched(
        &mut history,
        Command::SetPivots {
            id: parent,
            rotate_pivot: None,
            scale_pivot: Some([1.0e10, 0.0, 0.0]),
            preserve: false,
        },
    );
}

fn group_options(preserve_position: bool) -> crate::native_scene::GroupOptions {
    use crate::native_scene::{GroupPivot, GroupUnder};
    crate::native_scene::GroupOptions {
        under: GroupUnder::World,
        pivot: GroupPivot::Origin,
        preserve_position,
    }
}

// The parent's 1e-3 scale keeps the child at 1e38 in world space; leaving it unscaled puts it
// past f32.
#[test]
fn group_without_preserve_position_and_ungroup_keep_local_that_leave_f32_are_refused() {
    use crate::native_scene::{GroupUnder, UngroupOptions};
    let (mut scene, parent) = test_fixtures::cube();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    scene.set_channel(parent, Channel::ScaleX, 1.0e-3).unwrap();
    scene
        .set_channel(child, Channel::TranslateX, 1.0e41)
        .unwrap();
    scene
        .reparent(child, Some(parent), ReparentKeep::Local)
        .unwrap();
    let mut history = History::new(scene);
    refused_and_untouched(
        &mut history,
        Command::Group {
            ids: vec![child],
            options: group_options(false),
        },
    );
    // With Preserve Position the world position is kept, so the group is fine.
    history
        .execute(Command::Group {
            ids: vec![child],
            options: group_options(true),
        })
        .unwrap();
    let group = history.scene().nodes().last().unwrap().id();
    history
        .execute(Command::EditNode {
            id: group,
            name: None,
            channels: channel(Channel::ScaleX, 1.0e-3),
        })
        .unwrap();
    edit(&mut history, child, channel(Channel::TranslateX, 1.0e41)).unwrap();
    refused_and_untouched(
        &mut history,
        Command::Ungroup {
            ids: vec![group],
            options: UngroupOptions {
                under: GroupUnder::World,
                keep: ReparentKeep::Local,
            },
        },
    );
}

// The cap moves 1e10 cm in the mesh frame; the parent's 1e30 scale carries it to 1e40 in world
// space, past f32::MAX, while the parent matrix stays finite.
#[test]
fn an_extrusion_under_a_huge_scale_that_leaves_f32_is_refused() {
    let (mut scene, parent) = test_fixtures::cube();
    let child = scene.create(Primitive::Cube, "child").unwrap();
    for axis in [Channel::ScaleX, Channel::ScaleY, Channel::ScaleZ] {
        scene.set_channel(parent, axis, 1.0e30).unwrap();
    }
    scene
        .reparent(child, Some(parent), ReparentKeep::Local)
        .unwrap();
    let shape = scene.shapes().last().unwrap().id();
    let mut history = History::new(scene);
    let extrude = |distance| Command::ExtrudeFace {
        shape,
        face_index: 1,
        distance,
    };
    refused_and_untouched(&mut history, extrude(1.0e10));
    history.execute(extrude(1.0)).unwrap();
}

#[test]
fn an_extrusion_in_an_already_undrawable_scene_is_not_blocked() {
    let (mut scene, node) = test_fixtures::cube();
    scene.set_channel(node, Channel::ScaleX, 1.0e300).unwrap();
    let shape = scene.shapes()[0].id();
    let mut history = History::new(scene);
    history
        .execute(Command::ExtrudeFace {
            shape,
            face_index: 1,
            distance: 1.0,
        })
        .unwrap();
}
