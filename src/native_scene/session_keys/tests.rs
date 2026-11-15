use super::*;
use crate::native_scene::{AngularUnit, Channel, NativeTransform, ascii};
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

#[test]
fn a_refused_evaluation_leaves_the_key_table_and_scene_unchanged() {
    let (mut scene, id) = keyed_cube();
    let before = scene.clone();
    assert_eq!(
        scene.set_keys(&[id], 8, FX, MAX_FRAME + 1),
        Err(NativeError::InvalidFrame)
    );
    assert_eq!(scene, before);
    assert_eq!(
        scene.delete_keys(&[id], Some(1), FX, MAX_FRAME + 1),
        Err(NativeError::InvalidFrame)
    );
    assert_eq!(scene, before);
}

#[test]
fn an_empty_channel_mask_keys_nothing_and_leaves_a_valid_table() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    let before = scene.clone();
    scene.set_keys(&[id], 1, [false; 9], 1).unwrap();
    assert_eq!(scene.session_key_count(), 0);
    assert_eq!(scene, before);
    scene.session_keys.validate(&scene.nodes, false).unwrap();
}

#[test]
fn deleting_nothing_does_not_re_evaluate_over_a_held_edit() {
    let (mut scene, id) = keyed_cube();
    evaluate_at(&mut scene, 3);
    scene.set_channel(id, Channel::TranslateX, 99.0).unwrap();
    let before = scene.clone();
    // No key at frame 9, none on the other channels, and an unkeyed node's call is a no-op too.
    scene.delete_keys(&[id], Some(9), FX, 3).unwrap();
    assert_eq!(scene, before);
    assert_eq!(tx(&scene, id), 99.0);
    let other = cube(&mut scene, "b");
    let before = scene.clone();
    scene.delete_keys(&[other], None, ALL_CHANNELS, 3).unwrap();
    assert_eq!(scene, before);
    scene.delete_keys(&[id], Some(1), [false; 9], 3).unwrap();
    assert_eq!(scene, before.clone());
    assert_eq!(tx(&scene, id), 99.0);
}

#[test]
fn an_empty_table_still_refuses_a_frame_outside_the_bound() {
    let scene = NativeScene::default();
    assert!(scene.evaluated_writes(1).unwrap().is_empty());
    assert!(matches!(
        scene.evaluated_writes(MAX_FRAME + 1),
        Err(NativeError::InvalidFrame)
    ));
    let mut history = crate::history::History::new(scene);
    assert!(history.apply_evaluated(i32::MAX).is_err());
    assert_eq!(history.pose_frame(), DEFAULT_FRAME);
}

const RY: ChannelMask = [false, false, false, false, true, false, false, false, false];
const RYZ: ChannelMask = [false, false, false, false, true, true, false, false, false];

fn key_rotation(scene: &mut NativeScene, id: NodeId, frame: i32, value: f64) {
    scene.set_channel(id, Channel::RotateY, value).unwrap();
    scene.set_keys(&[id], frame, RY, frame).unwrap();
}

fn ry_values(scene: &NativeScene, id: NodeId) -> Vec<f64> {
    scene.key_rows(Some(id)).map(|row| row.value).collect()
}

#[test]
fn the_euler_filter_unrolls_a_flip_without_changing_any_orientation() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    for (frame, value) in [(1, 10.0), (2, 350.0), (3, 20.0), (4, 700.0)] {
        key_rotation(&mut scene, id, frame, value);
    }
    let before = ry_values(&scene, id);
    scene.euler_filter(&[id], RY, 4).unwrap();
    let after = ry_values(&scene, id);
    assert_eq!(after, vec![10.0, -10.0, 20.0, -20.0]);
    for (old, new) in before.iter().zip(&after) {
        let (old, new) = (old.to_radians(), new.to_radians());
        assert!((old.sin() - new.sin()).abs() < 1e-12 && (old.cos() - new.cos()).abs() < 1e-12);
    }
    assert_eq!(scene.node(id).unwrap().transform().values()[4], -20.0);
    let once = scene.clone();
    scene.euler_filter(&[id], RY, 4).unwrap();
    assert_eq!(scene, once);
}

#[test]
fn the_euler_filter_keeps_a_half_turn_and_follows_the_angular_unit() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    key_rotation(&mut scene, id, 1, 0.0);
    key_rotation(&mut scene, id, 2, 180.0);
    key_rotation(&mut scene, id, 3, -180.0);
    scene.euler_filter(&[id], RY, 3).unwrap();
    assert_eq!(ry_values(&scene, id), vec![0.0, 180.0, 180.0]);
    let mut radians = NativeScene::default();
    radians.units.angular = AngularUnit::Radian;
    let id = cube(&mut radians, "r");
    key_rotation(&mut radians, id, 1, 0.5);
    key_rotation(&mut radians, id, 2, 0.5 + std::f64::consts::TAU);
    radians.euler_filter(&[id], RY, 2).unwrap();
    let values = ry_values(&radians, id);
    assert!((values[1] - 0.5).abs() < 1e-12, "{values:?}");
}

#[test]
fn the_euler_filter_leaves_other_channels_and_nodes_alone() {
    let mut scene = NativeScene::default();
    let (a, b) = (cube(&mut scene, "a"), cube(&mut scene, "b"));
    for id in [a, b] {
        key_rotation(&mut scene, id, 1, 10.0);
        key_rotation(&mut scene, id, 2, 350.0);
    }
    scene.euler_filter(&[a, a], RYZ, 2).unwrap();
    assert_eq!(ry_values(&scene, a), vec![10.0, -10.0]);
    assert_eq!(ry_values(&scene, b), vec![10.0, 350.0]);
}

#[test]
fn a_refused_euler_filter_leaves_the_scene_untouched() {
    let mut scene = NativeScene::default();
    let (a, b) = (cube(&mut scene, "a"), cube(&mut scene, "b"));
    key_rotation(&mut scene, a, 1, 10.0);
    key_rotation(&mut scene, a, 2, 350.0);
    scene.set_channel(a, Channel::TranslateX, 3.0).unwrap();
    scene.set_keys(&[a], 1, FX, 2).unwrap();
    let before = scene.clone();
    let mut mixed = RY;
    mixed[0] = true;
    for (nodes, mask, frame, error) in [
        (vec![a], mixed, 2, NativeError::EulerNotRotation),
        (vec![a], FX, 2, NativeError::EulerNotRotation),
        (vec![a], [false; 9], 2, NativeError::EulerNoRotationKeys),
        (vec![b], RY, 2, NativeError::EulerNoRotationKeys),
        (vec![], RY, 2, NativeError::EulerNoRotationKeys),
        (
            vec![a],
            [false, false, false, true, false, false, false, false, false],
            2,
            NativeError::EulerNoRotationKeys,
        ),
        (vec![a], RY, MAX_FRAME + 1, NativeError::InvalidFrame),
    ] {
        assert_eq!(scene.euler_filter(&nodes, mask, frame), Err(error));
        assert_eq!(scene, before);
    }
    let missing = NodeId::from_decimal("999").unwrap();
    assert!(scene.euler_filter(&[a, missing], RY, 2).is_err());
    assert_eq!(scene, before);
}

#[test]
fn the_euler_filter_refuses_a_key_too_large_to_shift_and_an_unsupported_unit() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    key_rotation(&mut scene, id, 1, 0.0);
    key_rotation(&mut scene, id, 2, f64::MAX);
    let before = scene.clone();
    assert_eq!(
        scene.euler_filter(&[id], RY, 2),
        Err(NativeError::EulerValueTooLarge)
    );
    assert_eq!(scene, before);
    let mut grads = before.clone();
    grads.units.angular = AngularUnit::Unsupported("grad".into());
    let kept = grads.clone();
    assert!(grads.euler_filter(&[id], RY, 2).is_err());
    assert_eq!(grads, kept);
}

const RXYZ: ChannelMask = [false, false, false, true, true, true, false, false, false];

fn key_triple(scene: &mut NativeScene, id: NodeId, frame: i32, triple: [f64; 3]) {
    for (channel, value) in [Channel::RotateX, Channel::RotateY, Channel::RotateZ]
        .into_iter()
        .zip(triple)
    {
        scene.set_channel(id, channel, value).unwrap();
    }
    scene.set_keys(&[id], frame, RXYZ, frame).unwrap();
}

fn triples(scene: &NativeScene, id: NodeId) -> Vec<[f64; 3]> {
    let rows: Vec<KeyRow> = scene.key_rows(Some(id)).collect();
    let last = rows.iter().map(|row| row.frame).max().unwrap_or(0);
    let mut out = vec![[0.0; 3]; last as usize];
    for row in rows
        .iter()
        .filter(|row| row.channel >= 3 && row.channel < 6)
    {
        out[(row.frame - 1) as usize][row.channel - 3] = row.value;
    }
    out
}

fn matrix(order: u8, triple: [f64; 3], radians: bool) -> glam::DMat4 {
    let angles = triple.map(|value| if radians { value } else { value.to_radians() });
    crate::native_scene::transform_extras::rotation_matrix(order, angles).unwrap()
}

/// A camera panning through 90 degrees on its middle axis: the solver reports the equivalent
/// flipped triple after the pan, and one key a whole turn away on top of that.
#[test]
fn the_euler_filter_removes_a_triple_flip_for_every_rotate_order_and_unit() {
    for radians in [false, true] {
        let (turn, scale) = if radians {
            (std::f64::consts::TAU, 1f64.to_radians())
        } else {
            (360.0, 1.0)
        };
        let half = turn / 2.0;
        for order in 0..6u8 {
            let axes = crate::native_scene::transform_extras::order_axes(order).unwrap();
            let smooth = [
                [10.0, 80.0, 5.0],
                [12.0, 85.0, 6.0],
                [14.0, 88.0, 7.0],
                [16.0, 89.0, 8.0],
            ]
            .map(|triple| triple.map(|value| value * scale));
            let mut solved = smooth;
            for (index, triple) in solved.iter_mut().enumerate().skip(1) {
                triple[axes[0]] += half;
                triple[axes[1]] = half - triple[axes[1]];
                triple[axes[2]] += half;
                if index == 3 {
                    triple[axes[0]] += turn;
                }
            }
            let mut scene = NativeScene::default();
            scene.units.angular = if radians {
                AngularUnit::Radian
            } else {
                AngularUnit::Degree
            };
            let id = cube(&mut scene, "cam");
            for (frame, triple) in (1..).zip(solved) {
                key_triple(&mut scene, id, frame, triple);
            }
            scene.nodes[0].extras.rotate_order = Some(order);
            scene.euler_filter(&[id], RXYZ, 4).unwrap();
            let filtered = triples(&scene, id);
            for (index, (got, want)) in filtered.iter().zip(&smooth).enumerate() {
                for axis in 0..3 {
                    assert!(
                        (got[axis] - want[axis]).abs() < 1e-9,
                        "order {order} radians {radians} key {index}: {got:?} vs {want:?}"
                    );
                }
                let (a, b) = (
                    matrix(order, solved[index], radians),
                    matrix(order, *got, radians),
                );
                assert!(
                    a.abs_diff_eq(b, 1e-9),
                    "orientation changed: order {order} radians {radians} key {index}"
                );
            }
            let once = scene.clone();
            scene.euler_filter(&[id], RXYZ, 4).unwrap();
            assert_eq!(scene, once, "order {order} radians {radians}");
        }
    }
}

#[test]
fn a_partial_mask_or_mismatched_frames_unroll_each_curve_without_a_flip() {
    let mut scene = NativeScene::default();
    let id = cube(&mut scene, "a");
    key_triple(&mut scene, id, 1, [10.0, 80.0, 5.0]);
    key_triple(&mut scene, id, 2, [190.0, 100.0, 185.0]);
    let partial = [false, false, false, true, true, false, false, false, false];
    scene.euler_filter(&[id], partial, 2).unwrap();
    assert_eq!(triples(&scene, id)[1], [190.0, 100.0, 185.0]);
    key_rotation(&mut scene, id, 3, 99.0);
    let mut mismatched = scene.clone();
    mismatched.euler_filter(&[id], RXYZ, 3).unwrap();
    assert_eq!(mismatched.key_rows(Some(id)).count(), 7);
    assert_eq!(triples(&mismatched, id)[1], [190.0, 100.0, 185.0]);
}

#[test]
fn filtering_twice_changes_nothing_in_either_unit_even_at_half_a_turn() {
    for (unit, turn) in [
        (AngularUnit::Degree, 360.0),
        (AngularUnit::Radian, std::f64::consts::TAU),
    ] {
        let half = turn / 2.0;
        let mut scene = NativeScene::default();
        scene.units.angular = unit;
        let id = cube(&mut scene, "a");
        for (frame, value) in [
            (1, 0.1),
            (2, 0.1 + half),
            (3, 0.1 + 2.0 * half),
            (4, 0.1 - 3.0 * turn),
            (5, 0.1 + half),
        ] {
            key_rotation(&mut scene, id, frame, value);
        }
        scene.euler_filter(&[id], RY, 5).unwrap();
        let once = scene.clone();
        scene.euler_filter(&[id], RY, 5).unwrap();
        assert_eq!(scene, once);
    }
}

const PRE_POST: [Infinity; 5] = [
    Infinity::Constant,
    Infinity::Linear,
    Infinity::Cycle,
    Infinity::CycleRelative,
    Infinity::Oscillate,
];

/// A bent curve: 2 at frame 1, 8 at 3 and 10 at 5, so the first segment rises 3 a frame, the last
/// rises 1, the span is 4 and the whole span rises 8. No two modes agree beyond the keys.
fn infinity_curve() -> Curve {
    Curve::from([(1, 2.0), (3, 8.0), (5, 10.0)])
}

#[test]
fn every_infinity_mode_continues_the_curve_beyond_its_keys() {
    use Infinity::*;
    let curve = infinity_curve();
    let at = |pre, post, frame| evaluate_with(&curve, frame, (pre, post)).unwrap();
    // Inside and at the keys the modes make no difference.
    for mode in PRE_POST {
        for (frame, value) in [(1, 2.0), (2, 5.0), (3, 8.0), (4, 9.0), (5, 10.0)] {
            assert_eq!(at(mode, mode, frame), value, "{mode:?} at {frame}");
        }
    }
    // Post side: (mode, [(frame, value)]), several periods out.
    for (mode, rows) in [
        (Constant, [(6, 10.0), (9, 10.0), (13, 10.0), (14, 10.0)]),
        // The last segment's slope (1), not the first's (3).
        (Linear, [(6, 11.0), (9, 14.0), (13, 18.0), (14, 19.0)]),
        (Cycle, [(6, 5.0), (9, 2.0), (13, 2.0), (14, 5.0)]),
        (
            CycleRelative,
            [(6, 13.0), (9, 18.0), (13, 26.0), (14, 29.0)],
        ),
        (Oscillate, [(6, 9.0), (9, 2.0), (13, 10.0), (14, 9.0)]),
    ] {
        for (frame, value) in rows {
            assert_eq!(at(Constant, mode, frame), value, "post {mode:?} at {frame}");
        }
    }
    for (frame, value) in [(7, 8.0), (10, 5.0), (11, 8.0)] {
        assert_eq!(at(Constant, Cycle, frame), value, "post cycle at {frame}");
        assert_eq!(
            at(Constant, Oscillate, frame),
            value,
            "post oscillate at {frame}"
        );
    }
    assert_eq!(at(Constant, CycleRelative, 7), 16.0);
    assert_eq!(at(Constant, CycleRelative, 10), 21.0);
    // Pre side, several periods out: the first segment's slope (3), not the last's.
    for (mode, rows) in [
        (Constant, [(0, 2.0), (-1, 2.0), (-3, 2.0), (-4, 2.0)]),
        (Linear, [(0, -1.0), (-1, -4.0), (-3, -10.0), (-4, -13.0)]),
        (Cycle, [(0, 9.0), (-1, 8.0), (-3, 2.0), (-4, 9.0)]),
        (CycleRelative, [(0, 1.0), (-1, 0.0), (-3, -6.0), (-4, -7.0)]),
        (Oscillate, [(0, 5.0), (-1, 8.0), (-3, 10.0), (-4, 9.0)]),
    ] {
        for (frame, value) in rows {
            assert_eq!(at(mode, Constant, frame), value, "pre {mode:?} at {frame}");
        }
    }
    // Each side reads only its own mode.
    assert_eq!(at(Constant, Linear, 0), 2.0);
    assert_eq!(at(Linear, Constant, 9), 10.0);
    assert_eq!(at(Cycle, CycleRelative, 0), 9.0);
    assert_eq!(at(Cycle, CycleRelative, 6), 13.0);
}

#[test]
fn a_single_key_or_a_missing_curve_holds_under_every_mode() {
    let one = Curve::from([(4, 3.5)]);
    for mode in PRE_POST {
        for frame in [-1_000_000, 0, 4, 5, 1_000_000] {
            assert_eq!(evaluate_with(&one, frame, (mode, mode)), Some(3.5));
        }
        assert_eq!(evaluate_with(&Curve::new(), 1, (mode, mode)), None);
    }
}

#[test]
fn set_infinity_changes_evaluation_reports_back_and_re_evaluates_the_channel() {
    let (mut scene, id) = keyed_cube();
    assert_eq!(
        scene.infinity_of(id, 0),
        (Infinity::Constant, Infinity::Constant)
    );
    scene
        .set_infinity(&[id], FX, Some(Infinity::Linear), Some(Infinity::Cycle), 7)
        .unwrap();
    assert_eq!(
        scene.infinity_of(id, 0),
        (Infinity::Linear, Infinity::Cycle)
    );
    // Frame 7 is two past the last key: cycle of span 4 reads frame 3, which holds 6.
    assert_eq!(tx(&scene, id), 6.0);
    evaluate_at(&mut scene, -1);
    assert_eq!(tx(&scene, id), -2.0);
    // Only one side given leaves the other alone.
    scene
        .set_infinity(&[id], FX, None, Some(Infinity::Oscillate), 1)
        .unwrap();
    assert_eq!(
        scene.infinity_of(id, 0),
        (Infinity::Linear, Infinity::Oscillate)
    );
    // Setting the same values again is unchanged.
    let once = scene.clone();
    scene
        .set_infinity(&[id], FX, Some(Infinity::Linear), None, 1)
        .unwrap();
    assert_eq!(scene, once);
    // The setting is part of the table's equality and returns to the default exactly.
    scene
        .set_infinity(
            &[id],
            FX,
            Some(Infinity::Constant),
            Some(Infinity::Constant),
            1,
        )
        .unwrap();
    let mut plain = keyed_cube().0;
    evaluate_at(&mut plain, 1);
    assert!(scene.session_keys == plain.session_keys);
    assert!(scene.session_keys != once.session_keys);
}

#[test]
fn infinity_is_per_curve_and_dies_with_the_curves_last_key() {
    let (mut scene, id) = keyed_cube();
    scene.set_channel(id, Channel::TranslateY, 1.0).unwrap();
    let fy = [false, true, false, false, false, false, false, false, false];
    scene.set_keys(&[id], 1, fy, 1).unwrap();
    scene
        .set_infinity(&[id], FX, Some(Infinity::Cycle), None, 1)
        .unwrap();
    assert_eq!(scene.infinity_of(id, 0).0, Infinity::Cycle);
    assert_eq!(scene.infinity_of(id, 1).0, Infinity::Constant);
    scene.delete_keys(&[id], None, FX, 1).unwrap();
    assert_eq!(
        scene.infinity_of(id, 0),
        (Infinity::Constant, Infinity::Constant)
    );
    assert!(scene.validate().is_ok());
    // Keying the channel again starts from the default, not the forgotten setting.
    scene.set_keys(&[id], 2, FX, 2).unwrap();
    assert_eq!(scene.infinity_of(id, 0).0, Infinity::Constant);
    scene
        .set_infinity(&[id], FX, Some(Infinity::Linear), None, 2)
        .unwrap();
    scene.remove(id).unwrap();
    assert!(scene.session_keys.infinity.is_empty());
    assert!(scene.validate().is_ok());
}

#[test]
fn refused_infinity_changes_nothing() {
    let (mut scene, id) = keyed_cube();
    let other = cube(&mut scene, "b");
    let fy = [false, true, false, false, false, false, false, false, false];
    let before = scene.clone();
    for (nodes, mask, frame, error) in [
        (vec![id], fy, 1, NativeError::InfinityNoKeys),
        (vec![other], FX, 1, NativeError::InfinityNoKeys),
        (vec![], FX, 1, NativeError::InfinityNoKeys),
        (vec![id], FX, MAX_FRAME + 1, NativeError::InvalidFrame),
    ] {
        let result = scene.set_infinity(&nodes, mask, Some(Infinity::Cycle), None, frame);
        assert_eq!(result, Err(error));
        assert_eq!(scene, before);
    }
    let missing = NodeId::from_decimal("999").unwrap();
    assert!(
        scene
            .set_infinity(&[id, missing], FX, Some(Infinity::Cycle), None, 1)
            .is_err()
    );
    assert_eq!(scene, before);
    let mut bad = before.clone();
    bad.session_keys
        .infinity
        .insert((id, 1), (Infinity::Cycle, Infinity::Cycle));
    assert_eq!(bad.validate(), Err(NativeError::InvalidSessionKeys));
    let mut default = before.clone();
    default
        .session_keys
        .infinity
        .insert((id, 0), (Infinity::Constant, Infinity::Constant));
    assert_eq!(default.validate(), Err(NativeError::InvalidSessionKeys));
}
