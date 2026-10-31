use super::*;
use crate::native_scene::{Channel, NativeTransform, ascii};
use crate::scene::Primitive;

const FX: ChannelMask = [true, false, false, false, false, false, false, false, false];

fn cube(scene: &mut NativeScene, name: &str) -> NodeId {
    scene.create(Primitive::Cube, name).unwrap()
}

fn tx(scene: &NativeScene, id: NodeId) -> f64 {
    scene.node(id).unwrap().transform().translation[0]
}

fn keyed_cube() -> (NativeScene, NodeId) {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    scene.set_channel(id, Channel::TranslateX, 2.0).unwrap();
    scene.set_keys(&[id], 1, FX, 1).unwrap();
    scene.set_channel(id, Channel::TranslateX, 10.0).unwrap();
    scene.set_keys(&[id], 5, FX, 5).unwrap();
    (scene, id)
}

fn evaluate_at(scene: &mut NativeScene, frame: i32) {
    let writes = scene.evaluated_writes(frame).unwrap();
    scene.apply_evaluated(writes);
}

#[test]
fn a_frame_must_be_finite_whole_and_inside_the_bound() {
    assert_eq!(whole_frame(12.0), Ok(12));
    assert_eq!(whole_frame(-0.0), Ok(0));
    assert_eq!(whole_frame(f64::from(MAX_FRAME)), Ok(MAX_FRAME));
    assert_eq!(whole_frame(f64::from(-MAX_FRAME)), Ok(-MAX_FRAME));
    for time in [
        0.5,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from(MAX_FRAME) + 1.0,
        f64::from(-MAX_FRAME) - 1.0,
        f64::MAX,
    ] {
        assert_eq!(whole_frame(time), Err(NativeError::InvalidFrame), "{time}");
    }
}

#[test]
fn evaluation_interpolates_between_keys_and_holds_outside_them() {
    let (mut scene, id) = keyed_cube();
    for (frame, expected) in [(-3, 2.0), (1, 2.0), (3, 6.0), (5, 10.0), (90, 10.0)] {
        evaluate_at(&mut scene, frame);
        assert_eq!(tx(&scene, id), expected, "frame {frame}");
    }
    assert_eq!(scene.session_key_count(), 2);
}

#[test]
fn a_key_frame_returns_the_exact_key_value() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    let value = 0.1 + 0.2;
    scene.set_channel(id, Channel::TranslateX, value).unwrap();
    scene.set_keys(&[id], 1, FX, 1).unwrap();
    scene.set_channel(id, Channel::TranslateX, 7.0).unwrap();
    scene.set_keys(&[id], 3, FX, 3).unwrap();
    evaluate_at(&mut scene, 2);
    evaluate_at(&mut scene, 1);
    assert_eq!(tx(&scene, id).to_bits(), value.to_bits());
}

#[test]
fn interpolation_between_the_largest_values_stays_finite() {
    let curve = Curve::from([(0, f64::MAX), (2, -f64::MAX)]);
    assert_eq!(evaluate(&curve, 1), Some(0.0));
    let same = Curve::from([(0, f64::MAX), (3, f64::MAX)]);
    assert!(evaluate(&same, 1).unwrap().is_finite());
}

#[test]
fn set_keys_covers_all_channels_and_each_node_once() {
    let mut scene = NativeScene::default();
    let a = cube(&mut scene, "a");
    let b = cube(&mut scene, "b");
    scene.set_keys(&[a, b, a], 1, ALL_CHANNELS, 1).unwrap();
    assert_eq!(scene.session_key_count(), 18);
    assert_eq!(scene.key_frames(&[a]), BTreeSet::from([1]));
    let rows: Vec<_> = scene.key_rows(Some(b)).collect();
    assert_eq!(rows.len(), 9);
    assert_eq!(rows[0].channel, 0);
    assert_eq!(rows[8].value, 1.0);
}

#[test]
fn a_key_at_another_frame_evaluates_the_touched_channel_at_the_current_frame() {
    let (mut scene, id) = keyed_cube();
    // At frame 3 the pose is 6; a key at frame 9 of the held value leaves the curve at 6 there.
    evaluate_at(&mut scene, 3);
    scene.set_channel(id, Channel::TranslateX, 40.0).unwrap();
    scene.set_keys(&[id], 9, FX, 3).unwrap();
    assert_eq!(tx(&scene, id), 6.0);
    evaluate_at(&mut scene, 9);
    assert_eq!(tx(&scene, id), 40.0);
}

#[test]
fn a_key_on_one_channel_leaves_a_held_edit_on_another_alone() {
    let (mut scene, id) = keyed_cube();
    let ty_mask = [false, true, false, false, false, false, false, false, false];
    scene.set_keys(&[id], 5, ty_mask, 5).unwrap();
    scene.set_channel(id, Channel::TranslateX, 33.0).unwrap();
    scene.set_keys(&[id], 5, ty_mask, 5).unwrap();
    assert_eq!(tx(&scene, id), 33.0);
    evaluate_at(&mut scene, 6);
    assert_eq!(tx(&scene, id), 10.0);
}

#[test]
fn deleting_keys_evaluates_what_remains_and_deleting_all_leaves_the_pose() {
    let (mut scene, id) = keyed_cube();
    evaluate_at(&mut scene, 3);
    scene.delete_keys(&[id], Some(5), FX, 3).unwrap();
    assert_eq!(tx(&scene, id), 2.0);
    scene.set_channel(id, Channel::TranslateX, 8.0).unwrap();
    scene.delete_keys(&[id], None, ALL_CHANNELS, 3).unwrap();
    assert_eq!(scene.session_key_count(), 0);
    assert_eq!(tx(&scene, id), 8.0);
    evaluate_at(&mut scene, 40);
    assert_eq!(tx(&scene, id), 8.0);
}

#[test]
fn deleting_a_frame_without_keys_changes_nothing() {
    let (mut scene, id) = keyed_cube();
    let before = scene.clone();
    scene.delete_keys(&[id], Some(77), ALL_CHANNELS, 5).unwrap();
    assert_eq!(scene, before);
}

#[test]
fn removing_a_node_prunes_its_keys() {
    let (mut scene, id) = keyed_cube();
    let other = cube(&mut scene, "b");
    scene.set_keys(&[other], 1, FX, 1).unwrap();
    scene.remove(id).unwrap();
    assert_eq!(scene.session_key_count(), 1);
    scene.validate().unwrap();
}

#[test]
fn a_key_table_at_its_bound_refuses_more_and_changes_nothing() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    let fill = MAX_SESSION_KEYS / 9;
    {
        let curves = scene.session_keys.curves.entry(id).or_default();
        for frame in 0..fill as i32 {
            for curve in curves.iter_mut() {
                curve.insert(frame, 0.0);
            }
        }
    }
    let spare = MAX_SESSION_KEYS - fill * 9;
    let curves = scene.session_keys.curves.get_mut(&id).unwrap();
    for frame in 0..spare as i32 {
        curves[0].insert(fill as i32 + frame, 0.0);
    }
    assert_eq!(scene.session_key_count(), MAX_SESSION_KEYS);
    let before = scene.clone();
    assert_eq!(
        scene.set_keys(&[id], 900_000, FX, 1),
        Err(NativeError::KeyLimit)
    );
    assert_eq!(scene, before);
    // Replacing a key adds none, so a full table still accepts it.
    scene.set_keys(&[id], 0, FX, 1).unwrap();
}

#[test]
fn keys_refuse_unknown_nodes_bad_frames_and_other_time_units() {
    let (mut scene, id) = keyed_cube();
    let before = scene.clone();
    let missing = NodeId::from_decimal("999").unwrap();
    assert!(matches!(
        scene.set_keys(&[id, missing], 8, FX, 1),
        Err(NativeError::Legacy(_))
    ));
    assert!(matches!(
        scene.delete_keys(&[id, missing], None, FX, 1),
        Err(NativeError::Legacy(_))
    ));
    assert_eq!(
        scene.set_keys(&[id], MAX_FRAME + 1, FX, 1),
        Err(NativeError::InvalidFrame)
    );
    assert_eq!(
        scene.delete_keys(&[id], Some(i32::MIN), FX, 1),
        Err(NativeError::InvalidFrame)
    );
    assert_eq!(scene, before);
    scene.units.time = "ntsc".into();
    assert_eq!(
        scene.set_keys(&[id], 8, FX, 1),
        Err(NativeError::UnsupportedTimeUnit)
    );
    assert_eq!(
        scene.delete_keys(&[id], None, FX, 1),
        Err(NativeError::UnsupportedTimeUnit)
    );
}

#[test]
fn sourced_scenes_refuse_keys_as_a_structural_edit() {
    let source = "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film; createNode transform -n \"root\";";
    let mut scene = ascii::decode(source.as_bytes().to_vec().into()).unwrap();
    let id = scene.nodes()[0].id();
    assert_eq!(
        scene.set_keys(&[id], 1, ALL_CHANNELS, 1),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(
        scene.delete_keys(&[id], None, ALL_CHANNELS, 1),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(scene.session_key_count(), 0);
    assert!(scene.evaluated_writes(5).unwrap().is_empty());
}

#[test]
fn validation_refuses_a_table_the_commands_cannot_build() {
    let (mut scene, id) = keyed_cube();
    scene.session_keys.curves.get_mut(&id).unwrap()[0].insert(2, f64::NAN);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionKeys));
    let (mut scene, id) = keyed_cube();
    scene.session_keys.curves.get_mut(&id).unwrap()[0].insert(MAX_FRAME + 1, 0.0);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionKeys));
    let (mut scene, id) = keyed_cube();
    let keys = scene.session_keys.curves.remove(&id).unwrap();
    scene
        .session_keys
        .curves
        .insert(NodeId::from_decimal("999").unwrap(), keys);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionKeys));
    let (mut scene, id) = keyed_cube();
    scene.session_keys.curves.insert(id, Default::default());
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionKeys));
}

#[test]
fn the_table_compares_bitwise_and_blocks_the_legacy_view() {
    let (scene, id) = keyed_cube();
    let mut other = scene.clone();
    other.session_keys.curves.get_mut(&id).unwrap()[0].insert(1, 2.0);
    assert_eq!(scene, other);
    let mut signed = NativeScene::default();
    let a = cube(&mut signed, "a");
    signed.set_channel(a, Channel::TranslateY, 0.0).unwrap();
    signed.set_keys(&[a], 1, ALL_CHANNELS, 1).unwrap();
    let mut flipped = signed.clone();
    flipped.session_keys.curves.get_mut(&a).unwrap()[1].insert(1, -0.0);
    assert_ne!(signed, flipped);
    assert_eq!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView("session keys"))
    );
}

#[test]
fn a_write_keeps_the_other_channels_of_the_node() {
    let (mut scene, id) = keyed_cube();
    let held = NativeTransform {
        translation: [10.0, 4.0, 5.0],
        ..NativeTransform::default()
    };
    scene.set_transform(id, held).unwrap();
    evaluate_at(&mut scene, 1);
    assert_eq!(
        scene.node(id).unwrap().transform().translation,
        [2.0, 4.0, 5.0]
    );
}
