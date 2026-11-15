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
