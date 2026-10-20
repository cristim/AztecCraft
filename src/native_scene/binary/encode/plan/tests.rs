use super::*;
use crate::{
    maya_binary::container,
    native_scene::{
        Scalar, ScalarOrigin,
        binary::{
            ErrorKind, decode, encode,
            tests::{
                site_fixture::{
                    SITE_AUX, double_aux, other_node_with_triple, sited_transform, triple_aux,
                },
                wire_fixture::{scene as wire_scene, scene_with_units},
            },
        },
    },
};
use std::sync::Arc;

const T: &[TransformField] = &[TransformField::Translation];

fn file(aux: u32) -> Vec<u8> {
    [
        sited_transform(b"a", &[]),
        sited_transform(b"b", &[triple_aux(b"r", SITE_AUX, [1.0; 3])]),
        other_node_with_triple(b"anchor", aux),
    ]
    .concat()
}

fn decoded(bytes: Arc<[u8]>) -> NativeScene {
    decode(bytes).unwrap()
}

fn edit(scene: &mut NativeScene, node: usize, component: usize, value: f64) {
    scene.nodes[node].channels[component] = Scalar {
        value,
        origin: None,
        replaced: true,
    };
}

fn run(scene: &NativeScene, fields: &[TransformField]) -> Result<Vec<Insertion>, BinaryError> {
    plan(scene, scene.source.as_ref().unwrap(), fields).map(|planned| planned.insertions)
}

/// Payload end of the `index`th transform group, from the container's chunk boundaries.
fn payload_end_of_transform(bytes: &Arc<[u8]>, index: usize) -> usize {
    let inspected = container::inspect(bytes.clone()).unwrap();
    let group = inspected
        .chunks()
        .iter()
        .filter(|c| c.form() == Some(*b"XFRM"))
        .nth(index)
        .unwrap();
    group.payload_range().end
}

#[test]
fn production_gate_admits_translation_only_and_saves_without_edits_stay_byte_identical() {
    assert_eq!(INSERTABLE, [TransformField::Translation]);
    for field in [TransformField::Rotation, TransformField::Scale] {
        assert!(!INSERTABLE.contains(&field));
    }
    let bytes = wire_scene(&file(SITE_AUX));
    let scene = decoded(bytes.clone());
    assert!(!scene.source.as_ref().unwrap().sites.is_empty());
    assert_eq!(encode(&scene).unwrap(), bytes.as_ref());
}

#[test]
fn build_record_is_the_exact_48_byte_chunk_with_the_given_aux() {
    let mut expected = b"DBL3".to_vec();
    expected.extend(SITE_AUX.to_be_bytes());
    expected.extend(27u64.to_be_bytes());
    expected.extend(b"t\0\x20");
    for word in [1.5f64, -0.0, 3.0] {
        expected.extend(word.to_be_bytes());
    }
    expected.extend([0; 5]);
    let record = build_record(TransformField::Translation, SITE_AUX, [1.5, -0.0, 3.0]).unwrap();
    assert_eq!(record, expected);
    assert_eq!(record, triple_aux(b"t", SITE_AUX, [1.5, -0.0, 3.0]));
    let error = build_record(TransformField::Rotation, SITE_AUX, [0.0; 3]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Unsupported);
}

#[test]
fn an_edited_absent_translation_becomes_one_record_at_the_recorded_site() {
    let bytes = wire_scene(&file(SITE_AUX));
    let mut scene = decoded(bytes.clone());
    edit(&mut scene, 0, 0, 5.0);
    let found = run(&scene, T).unwrap();
    assert_eq!(found.len(), 1);
    let site = scene
        .source
        .as_ref()
        .unwrap()
        .insert_site(scene.nodes[0].transform_id, TransformField::Translation)
        .unwrap();
    assert_eq!(found[0].at, payload_end_of_transform(&bytes, 0));
    assert_eq!(found[0].at, site.at);
    let retention = scene.source.as_ref().unwrap();
    assert!(
        retention
            .insert_site(scene.nodes[0].transform_id, TransformField::Rotation)
            .is_none()
    );
    assert_eq!(found[0].ancestors, site.ancestors);
    assert_eq!(found[0].bytes, triple_aux(b"t", SITE_AUX, [5.0, 0.0, 0.0]));
    assert_eq!(
        (found[0].owner, found[0].field),
        (scene.nodes[0].transform_id, TransformField::Translation)
    );
}

#[test]
fn only_replaced_components_carry_a_value_and_units_convert_to_centimetres() {
    // (declared unit, centimetres per unit as Units::linear_factor defines it)
    let units: [(&[u8], f64); 5] = [
        (b"cm", 1.0),
        (b"mm", 0.1),
        (b"m", 100.0),
        (b"in", 2.54),
        (b"ft", 30.48),
    ];
    for (unit, factor) in units {
        let mut scene = decoded(scene_with_units(&file(SITE_AUX), unit, b"deg"));
        edit(&mut scene, 0, 1, 25.0);
        let found = run(&scene, T).unwrap();
        let expected = triple_aux(b"t", SITE_AUX, [0.0, 25.0 * factor, 0.0]);
        assert_eq!(
            found[0].bytes,
            expected,
            "{}",
            String::from_utf8_lossy(unit)
        );
    }
}

#[test]
fn all_zero_words_insert_nothing_but_negative_zero_is_an_edit() {
    let mut scene = decoded(wire_scene(&file(SITE_AUX)));
    edit(&mut scene, 0, 0, 5.0);
    edit(&mut scene, 0, 0, 0.0);
    let planned = plan(&scene, scene.source.as_ref().unwrap(), T).unwrap();
    assert!(planned.insertions.is_empty());
    assert_eq!(
        planned.elided,
        [(scene.nodes[0].transform_id, TransformField::Translation)]
    );
    edit(&mut scene, 0, 2, -0.0);
    let found = run(&scene, T).unwrap();
    assert_eq!(found[0].bytes, triple_aux(b"t", SITE_AUX, [0.0, 0.0, -0.0]));
}

#[test]
fn unedited_absent_fields_and_fields_outside_the_gate_plan_nothing() {
    let mut scene = decoded(wire_scene(&file(SITE_AUX)));
    assert!(run(&scene, T).unwrap().is_empty());
    edit(&mut scene, 0, 0, 5.0);
    assert!(run(&scene, &[]).unwrap().is_empty());
    let mut rotated = decoded(wire_scene(&file(SITE_AUX)));
    edit(&mut rotated, 0, 3, 5.0);
    assert!(run(&rotated, T).unwrap().is_empty());
}

#[test]
fn mixed_origin_non_finite_and_siteless_edits_are_refused() {
    let mut mixed = decoded(wire_scene(&file(SITE_AUX)));
    edit(&mut mixed, 0, 0, 5.0);
    mixed.nodes[0].channels[1].origin = Some(ScalarOrigin::Binary {
        leaf: 0,
        component: 1,
    });
    assert_eq!(run(&mixed, T).unwrap_err().kind(), ErrorKind::Unsupported);

    let mut infinite = decoded(wire_scene(&file(SITE_AUX)));
    edit(&mut infinite, 0, 0, f64::INFINITY);
    assert!(run(&infinite, T).is_err());

    let mut siteless = decoded(wire_scene(&file(SITE_AUX ^ 1)));
    assert!(siteless.source.as_ref().unwrap().sites.is_empty());
    edit(&mut siteless, 0, 0, 5.0);
    assert_eq!(
        run(&siteless, T).unwrap_err().kind(),
        ErrorKind::Unsupported
    );

    // A node without a site is refused even when another node has one.
    let bytes = [
        sited_transform(b"a", &[]),
        sited_transform(
            b"c",
            &[
                triple_aux(b"r", SITE_AUX, [1.0; 3]),
                double_aux(b"v", 0x24, SITE_AUX, 1.0),
            ],
        ),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut partial = decoded(wire_scene(&bytes));
    assert_eq!(partial.source.as_ref().unwrap().sites.len(), 1);
    edit(&mut partial, 1, 0, 5.0);
    assert_eq!(run(&partial, T).unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn two_edited_nodes_plan_in_ascending_position() {
    let bytes = [
        sited_transform(b"a", &[]),
        sited_transform(b"b", &[]),
        other_node_with_triple(b"anchor", SITE_AUX),
    ]
    .concat();
    let mut scene = decoded(wire_scene(&bytes));
    edit(&mut scene, 1, 0, 2.0);
    edit(&mut scene, 0, 0, 1.0);
    let found = run(&scene, T).unwrap();
    assert_eq!(found.len(), 2);
    assert!(found[0].at < found[1].at);
    assert_eq!(found[0].owner, scene.nodes[0].transform_id);
}
