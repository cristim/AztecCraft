use super::*;
use crate::native_scene::{Channel, NativeScene, Scalar, test_fixtures::cube};
use crate::scene::NodeId;

fn binary(fields: &[(TransformField, [f64; 3])]) -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    let owner = scene.nodes[0].transform_id;
    let mut bytes = b"FOR8XFRM".to_vec();
    let mut records = std::iter::once(0..8).collect::<Vec<_>>();
    let mut leaves = Vec::new();
    for (field, raw) in fields {
        let header = bytes.len();
        bytes.extend_from_slice(b"DBL3hdr!");
        let payload = bytes.len();
        let key = match field {
            TransformField::Translation => "t",
            TransformField::Rotation => "r",
            TransformField::Scale => "s",
            TransformField::RotatePivot => "rp",
            TransformField::ScalePivot => "sp",
        };
        bytes.extend_from_slice(key.as_bytes());
        bytes.extend_from_slice(&[0, 0x20]);
        let numeric = bytes.len();
        for value in raw {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        let end = bytes.len();
        records.push(header..end);
        bytes.extend_from_slice(b"PAD");
        records.push(end..bytes.len());
        let slots = match field {
            TransformField::Translation => &mut scene.nodes[0].channels[..3],
            TransformField::Rotation => &mut scene.nodes[0].channels[3..6],
            TransformField::Scale => &mut scene.nodes[0].channels[6..],
            TransformField::RotatePivot => &mut scene.nodes[0].rotate_pivot[..],
            TransformField::ScalePivot => &mut scene.nodes[0].scale_pivot[..],
        };
        for (component, scalar) in slots.iter_mut().enumerate() {
            *scalar = Scalar {
                value: field
                    .storage()
                    .declared(raw[component], &scene.units)
                    .unwrap(),
                origin: Some(ScalarOrigin::Binary {
                    leaf: leaves.len(),
                    component,
                }),
                replaced: false,
            };
        }
        leaves.push(SourceLeaf {
            payload: payload..end,
            numeric: numeric..end,
            owner,
            field: *field,
            storage: field.storage(),
        });
    }
    scene.source = Some(SourceRetention {
        sites: Vec::new(),
        bytes: bytes.into(),
        format: SourceFormat::Binary,
        records,
        leaves,
        appearance: None,
    });
    scene.validate().unwrap();
    (scene, id)
}

#[test]
fn exact_source_coverage_and_typed_leaf_crosslinks_reject_invalid_candidates() {
    let (scene, _) = binary(&[(TransformField::Translation, [1.0, 2.0, 3.0])]);
    let mut bad = scene.clone();
    bad.source.as_mut().unwrap().records[1].start += 1;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.source.as_mut().unwrap().records[1].start -= 1;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.source.as_mut().unwrap().records.pop();
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    for case in 0..5 {
        bad = scene.clone();
        let source = bad.source.as_mut().unwrap();
        match case {
            0 => source.leaves[0].numeric.start = source.leaves[0].payload.start - 1,
            1 => source.leaves[0].numeric.end -= 1,
            2 => source.leaves[0].owner = NativeId(100),
            3 => source.leaves[0].storage = Storage::Radians,
            _ => source.leaves[0].payload.end = source.bytes.len() + 1,
        }
        assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    }
    bad = scene.clone();
    bad.nodes[0].channels[0].origin = Some(ScalarOrigin::Binary {
        leaf: 0,
        component: 1,
    });
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    let leaf = bad.source.as_ref().unwrap().leaves[0].clone();
    bad.source.as_mut().unwrap().leaves.push(leaf);
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.source.as_mut().unwrap().format = SourceFormat::Ascii;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.nodes[0].channels[0].origin = Some(ScalarOrigin::Ascii { literal: 0..3 });
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.nodes[0].rotate_pivot = std::array::from_fn(|i| bad.nodes[0].channels[i].clone());
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    let (mut other, _) = cube();
    other
        .create(crate::scene::Primitive::Cube, "other")
        .unwrap();
    other.nodes[0].channels[..3].clone_from_slice(&scene.nodes[0].channels[..3]);
    other.nodes[1].channels[..3].clone_from_slice(&scene.nodes[0].channels[..3]);
    other.source = scene.source.clone();
    assert_eq!(other.validate(), Err(NativeError::InvalidRetention));
    let (mut overlap, _) = binary(&[
        (TransformField::Translation, [1.0, 2.0, 3.0]),
        (TransformField::RotatePivot, [1.0, 2.0, 3.0]),
    ]);
    let source = overlap.source.as_mut().unwrap();
    source.leaves[1].numeric = source.leaves[0].numeric.clone();
    source.leaves[1].payload = source.leaves[0].payload.clone();
    assert_eq!(overlap.validate(), Err(NativeError::InvalidRetention));
}

#[test]
fn untouched_binary_rotation_and_pivots_use_original_internal_words_without_revalidation() {
    let raw = (1..10000)
        .map(|i| i as f64 / 7.0)
        .find(|raw| {
            let declared = Storage::Radians.declared(*raw, &Units::default()).unwrap();
            Storage::Radians
                .internal(declared, &Units::default())
                .unwrap()
                .to_bits()
                != raw.to_bits()
        })
        .unwrap();
    let (scene, id) = binary(&[
        (TransformField::Rotation, [raw, -0.0, 0.0]),
        (TransformField::RotatePivot, [1.000000000001, 2.0, -0.0]),
        (TransformField::ScalePivot, [-2.0, 0.0, 3.0]),
    ]);
    let validations = crate::native_scene::FULL_VALIDATIONS.with(|count| count.get());
    let internal = scene.internal_transform(id).unwrap();
    assert_eq!(internal.rotation_radians[0].to_bits(), raw.to_bits());
    assert_eq!(internal.rotation_radians[1].to_bits(), (-0.0f64).to_bits());
    assert_eq!(
        internal.rotate_pivot_cm[0].to_bits(),
        1.000000000001f64.to_bits()
    );
    assert_eq!(
        crate::native_scene::FULL_VALIDATIONS.with(|count| count.get()),
        validations
    );
    let mut bad = scene.clone();
    bad.nodes[0].channels[3].value = raw;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    bad = scene.clone();
    bad.nodes[0].channels[3].replaced = true;
    bad.nodes[0].channels[3].value = 0.0;
    bad.nodes[0].channels[4].replaced = true;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
}

#[test]
fn binary_word_noops_revert_and_signed_zero_preserve_complete_authoring_intent() {
    let units = Units::default();
    let (raw, alternative) = (1..10000)
        .find_map(|i| {
            let raw = i as f64 / 7.0;
            let declared = Storage::Radians.declared(raw, &units).unwrap();
            [declared.next_up(), declared.next_down()]
                .into_iter()
                .find(|value| {
                    Storage::Radians.internal(*value, &units).unwrap().to_bits() == raw.to_bits()
                })
                .map(|value| (raw, value))
        })
        .unwrap();
    let (mut scene, id) = binary(&[(TransformField::Rotation, [raw, 0.0, 0.0])]);
    let original = scene.clone();
    scene
        .set_channel(
            id,
            Channel::RotateX,
            scene.node(id).unwrap().transform().rotation[0],
        )
        .unwrap();
    assert_eq!(scene, original);
    scene
        .set_channel(id, Channel::RotateX, alternative)
        .unwrap();
    assert_eq!(scene, original);
    let (changed, same_word) = (1..10000)
        .find_map(|i| {
            let value = i as f64 / 11.0;
            let word = Storage::Radians.internal(value, &units).unwrap();
            [value.next_up(), value.next_down()]
                .into_iter()
                .find(|candidate| {
                    word.to_bits() != raw.to_bits()
                        && Storage::Radians
                            .internal(*candidate, &units)
                            .unwrap()
                            .to_bits()
                            == word.to_bits()
                })
                .map(|candidate| (value, candidate))
        })
        .unwrap();
    scene.set_channel(id, Channel::RotateX, 90.0).unwrap();
    assert_eq!(
        scene.internal_transform(id).unwrap().rotation_radians[0].to_bits(),
        std::f64::consts::FRAC_PI_2.to_bits()
    );
    scene.set_channel(id, Channel::RotateX, changed).unwrap();
    assert_ne!(scene, original);
    assert!(scene.nodes[0].channels[3].replaced);
    let edited = scene.clone();
    scene.set_channel(id, Channel::RotateX, same_word).unwrap();
    assert_eq!(scene, edited);
    scene
        .set_channel(id, Channel::RotateX, alternative)
        .unwrap();
    assert_eq!(scene, original);
    scene.set_channel(id, Channel::RotateY, -0.0).unwrap();
    assert_ne!(scene, original);
    assert_eq!(
        scene.internal_transform(id).unwrap().rotation_radians[1].to_bits(),
        (-0.0f64).to_bits()
    );
    scene.restore_authoring(original.clone());
    assert_eq!(scene, original);
    assert!(Arc::ptr_eq(
        &scene.source.as_ref().unwrap().bytes,
        &original.source.as_ref().unwrap().bytes
    ));
}

#[test]
fn absent_binary_records_and_late_transform_failure_do_not_mutate_earlier_slots() {
    let (mut scene, id) = binary(&[(TransformField::Translation, [1.000000000001, 0.0, 0.0])]);
    let before = scene.clone();
    scene
        .set_transform(id, scene.node(id).unwrap().transform())
        .unwrap();
    assert_eq!(scene, before);
    assert_eq!(
        scene.set_channel(id, Channel::ScaleZ, 2.0),
        Err(NativeError::UnsupportedEdit)
    );
    let mut transform = scene.node(id).unwrap().transform();
    transform.translation[0] = 2.000000000001;
    transform.scale[2] = 2.0;
    assert_eq!(
        scene.set_transform(id, transform),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, before);
    let mut bad = scene.clone();
    bad.nodes[0].channels[8].replaced = true;
    assert_eq!(bad.validate(), Err(NativeError::InvalidRetention));
    scene
        .set_channel(id, Channel::TranslateX, 2.000000000001)
        .unwrap();
    assert_eq!(
        scene.internal_transform(id).unwrap().translation_cm[0].to_bits(),
        2.000000000001f64.to_bits()
    );
    assert!(Arc::ptr_eq(
        &scene.source.as_ref().unwrap().bytes,
        &before.source.as_ref().unwrap().bytes
    ));
    assert_eq!(scene.nodes[0].channels[1..], before.nodes[0].channels[1..]);
    assert_eq!(scene.source, before.source);
}

#[test]
fn imported_debug_redacts_names_bytes_and_scalar_values() {
    let (mut scene, _) = binary(&[(TransformField::Translation, [1234.000000000001, 0.0, 0.0])]);
    scene.nodes[0].name = "private/path/asset".into();
    scene.shapes[0].name = "private-shape".into();
    for diagnostic in [
        format!("{scene:?}"),
        format!("{:?}", scene.nodes[0]),
        format!("{:?}", scene.shapes[0]),
        format!("{:?}", scene.nodes[0].channels[0]),
    ] {
        assert!(!diagnostic.contains("private"));
        assert!(!diagnostic.contains("1234"));
        assert!(!diagnostic.contains("FOR8"));
        assert!(diagnostic.len() < 512);
    }
}

#[test]
fn raw_binary_radians_reach_actual_view_before_f32_cancellation_and_edited_degrees_convert_once() {
    let units = Units::default();
    let (raw, translation) = (1..10000)
        .find_map(|index| {
            let raw = f64::from(index) / 7.0;
            let declared = Storage::Radians.declared(raw, &units).unwrap();
            let rounded = Storage::Radians.internal(declared, &units).unwrap();
            if raw.to_bits() == rounded.to_bits() {
                return None;
            }
            let (sine, cosine) = raw.sin_cos();
            let rotated_x = 0.5 * sine - 0.5 * cosine;
            let rotated_y = -0.5 * cosine - 0.5 * sine;
            let (wrong_sine, wrong_cosine) = rounded.sin_cos();
            let wrong_x = 0.5 * wrong_sine - 0.5 * wrong_cosine;
            let wrong_y = -0.5 * wrong_cosine - 0.5 * wrong_sine;
            (wrong_x != rotated_x || wrong_y != rotated_y)
                .then_some((raw, [-rotated_x, -rotated_y, 0.5]))
        })
        .unwrap();
    let (mut scene, id) = binary(&[
        (TransformField::Translation, translation),
        (TransformField::Rotation, [0.0, 0.0, raw]),
    ]);
    assert_eq!(
        scene.shapes()[0].mesh().effective_point_cm(0).unwrap(),
        [-0.5; 3]
    );
    let before = scene.clone();
    let view = crate::view_scene::ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    assert_eq!(view.instances()[0].world_vertices()[0], glam::Vec3::ZERO);
    assert_eq!(scene, before);
    scene.set_channel(id, Channel::RotateZ, 90.0).unwrap();
    let edited = crate::view_scene::ViewScene::from_native(&scene, 1024 * 1024).unwrap();
    let expected = glam::Vec3::new(
        (0.5 + translation[0]) as f32,
        (-0.5 + translation[1]) as f32,
        0.0,
    );
    assert!(edited.instances()[0].world_vertices()[0].distance(expected) < 1e-7);
}
