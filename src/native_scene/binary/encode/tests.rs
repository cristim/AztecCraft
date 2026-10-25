use super::super::tests::appearance_fixture;
use super::super::tests::wire_fixture::{self, creation, group, scene, triple};
use super::*;
use crate::{
    native_scene::{Channel, NativeTransform},
    view_scene::ViewScene,
};

fn fixture() -> (Arc<[u8]>, NativeScene) {
    let bytes: Arc<[u8]> = wire_fixture::specimen().into();
    let scene = super::super::decode(bytes.clone()).unwrap();
    (bytes, scene)
}

#[test]
fn appearance_metadata_survives_clone_and_single_authorized_word_patch() {
    let bytes: Arc<[u8]> = appearance_fixture::appearance_specimen().into();
    let mut candidate = super::super::decode(bytes.clone()).unwrap();
    assert_eq!(encode(&candidate).unwrap(), bytes.as_ref());
    let cloned = candidate.clone();
    assert!(Arc::ptr_eq(
        candidate
            .source
            .as_ref()
            .unwrap()
            .appearance
            .as_ref()
            .unwrap(),
        cloned.source.as_ref().unwrap().appearance.as_ref().unwrap()
    ));
    let id = candidate.nodes()[0].id();
    candidate
        .set_channel(id, Channel::TranslateX, 4.25)
        .unwrap();
    let output = encode(&candidate).unwrap();
    assert_only_word(&bytes, &output, numeric(&bytes, b"t"), 4.25);
    let reopened = super::super::decode(output.into()).unwrap();
    check_appearance_metadata(&candidate, &reopened).unwrap();
    assert_eq!(
        candidate.appearance_readiness(),
        reopened.appearance_readiness()
    );
    let before = candidate.clone();
    assert!(candidate.rename(id, "refused").is_err());
    assert_eq!(candidate, before);
}

#[test]
fn postdecode_appearance_guard_rejects_otherwise_valid_mismatching_metadata() {
    let expected = super::super::decode(appearance_fixture::appearance_specimen().into()).unwrap();
    let mut actual = expected.clone();
    let source = actual.source.as_mut().unwrap();
    let appearance = Arc::make_mut(source.appearance.as_mut().unwrap());
    appearance.materials[0].message_links.pop();
    actual.validate().unwrap();
    assert!(check_appearance_metadata(&expected, &actual).is_err());
}

#[test]
fn diffuse_origin_and_retained_canonical_capacity_are_decisive() {
    let expected = super::super::decode(appearance_fixture::appearance_specimen().into()).unwrap();
    let source = expected.source.as_ref().unwrap();
    let metadata = source
        .appearance
        .as_ref()
        .unwrap()
        .metadata_bytes()
        .unwrap();
    let state = expected.materials.capacity()
        * std::mem::size_of::<crate::native_scene::appearance::MaterialDiffuse>();
    assert!(state > 0);
    assert_eq!(
        retained_material_metadata(&expected).unwrap(),
        metadata + state
    );
    for owner in [false, true] {
        let mut changed = expected.clone();
        let appearance = Arc::make_mut(
            changed
                .source
                .as_mut()
                .unwrap()
                .appearance
                .as_mut()
                .unwrap(),
        );
        if owner {
            appearance.diffuse[0].shader += 1;
        } else {
            appearance.diffuse[0].span.value.start += 1;
        }
        assert!(changed.validate().is_err());
        assert!(encode(&changed).is_err());
    }
    let mut wrong_word = expected.clone();
    let wrong_span = wrong_word
        .source
        .as_ref()
        .unwrap()
        .appearance
        .as_ref()
        .unwrap()
        .materials[0]
        .inputs
        .iter()
        .find_map(|input| match input {
            crate::native_scene::appearance::InputEvidence::Saved {
                property: crate::native_scene::appearance::InputProperty::Reflectivity,
                span,
            } => Some(span.clone()),
            _ => None,
        })
        .unwrap();
    let appearance = Arc::make_mut(
        wrong_word
            .source
            .as_mut()
            .unwrap()
            .appearance
            .as_mut()
            .unwrap(),
    );
    appearance.diffuse[0].span = wrong_span;
    let source = wrong_word.source.as_ref().unwrap();
    let original = source.appearance.as_ref().unwrap().diffuse[0]
        .original(source)
        .unwrap();
    wrong_word.materials[0].value = original;
    wrong_word.materials[0].replaced = false;
    assert!(
        wrong_word.validate_materials().is_ok(),
        "wrong word must pass scalar-only checks"
    );
    assert!(
        wrong_word.validate().is_err(),
        "diffuse owner/input span must remain decisive"
    );
    assert!(encode(&wrong_word).is_err());
    let mut invalid = expected.clone();
    invalid.materials[0].binding += 1;
    assert!(invalid.validate().is_err());
    assert!(encode(&invalid).is_err());
}

fn numeric(bytes: &[u8], key: &[u8]) -> usize {
    let marker = [key, &[0, 0x20]].concat();
    bytes
        .windows(marker.len())
        .rposition(|v| v == marker)
        .unwrap()
        + marker.len()
}

fn assert_only_word(before: &[u8], after: &[u8], start: usize, value: f64) {
    assert_eq!(before.len(), after.len());
    assert!(
        after[..start] == before[..start],
        "untouched prefix differs"
    );
    assert!(
        after[start..start + 8] == value.to_be_bytes(),
        "replacement word differs"
    );
    assert!(
        after[start + 8..] == before[start + 8..],
        "untouched suffix differs"
    );
    assert!(
        before[start..start + 8] != after[start..start + 8],
        "replacement did not change word"
    );
}

#[test]
fn unedited_and_reverted_binary_are_byte_identical() {
    let (bytes, mut candidate) = fixture();
    assert_eq!(encode(&candidate).unwrap(), bytes.as_ref());
    let id = candidate.nodes()[0].id();
    let initial = candidate.node(id).unwrap().transform().translation[0];
    candidate
        .set_channel(id, Channel::TranslateX, initial + 0.25)
        .unwrap();
    candidate
        .set_channel(id, Channel::TranslateX, initial)
        .unwrap();
    assert_eq!(encode(&candidate).unwrap(), bytes.as_ref());
}

#[test]
fn precise_edit_replaces_only_its_authorized_word_and_reopens_in_view() {
    let (bytes, mut candidate) = fixture();
    let id = candidate.nodes()[0].id();
    let value = 16777217.125000004;
    candidate
        .set_channel(id, Channel::TranslateX, value)
        .unwrap();
    let output = encode(&candidate).unwrap();
    assert_only_word(&bytes, &output, numeric(&bytes, b"t"), value);
    let reopened = super::super::decode(output.into()).unwrap();
    assert_eq!(
        reopened.node(id).unwrap().transform().translation[0].to_bits(),
        value.to_bits()
    );
    assert_eq!(
        reopened.internal_transform(id).unwrap().rotation_radians[2].to_bits(),
        0.25f64.to_bits()
    );
    assert_eq!(reopened.shapes, candidate.shapes);
    assert_eq!(reopened.dag, candidate.dag);
    assert_eq!(reopened.units, candidate.units);
    assert_eq!(
        ViewScene::from_native(&reopened, 1024 * 1024)
            .unwrap()
            .instances()
            .len(),
        1
    );
    assert_eq!(
        candidate.source.as_ref().unwrap().bytes.as_ref(),
        bytes.as_ref()
    );
}

#[test]
fn angles_and_signed_zero_use_storage_words_without_touching_companions() {
    for (channel, value, key, component, expected) in [
        (
            Channel::RotateZ,
            90.0,
            b"r".as_slice(),
            2,
            std::f64::consts::FRAC_PI_2,
        ),
        (Channel::RotateX, -0.0, b"r".as_slice(), 0, -0.0),
    ] {
        let (bytes, mut candidate) = fixture();
        let id = candidate.nodes()[0].id();
        candidate.set_channel(id, channel, value).unwrap();
        let output = encode(&candidate).unwrap();
        assert_only_word(
            &bytes,
            &output,
            numeric(&bytes, key) + component * 8,
            expected,
        );
        let reopened = super::super::decode(output.into()).unwrap();
        assert_eq!(
            reopened.internal_transform(id).unwrap().rotation_radians[component].to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn declared_meter_and_radian_edits_preserve_internal_storage_units() {
    let bytes = wire_fixture::scene_with_units(
        &group(
            b"XFRM",
            &[
                creation(b"object", None),
                triple(b"t", [100.0, 200.0, 300.0]),
                triple(b"r", [0.125, 0.25, 0.5]),
            ]
            .concat(),
            4,
        ),
        b"m",
        b"rad",
    );
    let candidate = super::super::decode(bytes.clone()).unwrap();
    let id = candidate.nodes()[0].id();
    assert_eq!(
        candidate.node(id).unwrap().transform().translation,
        [1.0, 2.0, 3.0]
    );
    let mut translated = candidate.clone();
    translated
        .set_channel(id, Channel::TranslateX, 2.5)
        .unwrap();
    let output = encode(&translated).unwrap();
    assert_only_word(&bytes, &output, numeric(&bytes, b"t"), 250.0);
    let mut rotated = candidate;
    rotated.set_channel(id, Channel::RotateY, 0.75).unwrap();
    let output = encode(&rotated).unwrap();
    assert_only_word(&bytes, &output, numeric(&bytes, b"r") + 8, 0.75);
}

#[test]
fn only_effective_last_duplicate_assignment_changes() {
    let bytes = scene(&group(
        b"XFRM",
        &[
            creation(b"object", None),
            triple(b"t", [101.0, 102.0, 103.0]),
            triple(b"t", [1.0, 2.0, 3.0]),
        ]
        .concat(),
        4,
    ));
    let mut candidate = super::super::decode(bytes.clone()).unwrap();
    candidate
        .set_channel(candidate.nodes()[0].id(), Channel::TranslateY, 4.25)
        .unwrap();
    let output = encode(&candidate).unwrap();
    assert_only_word(&bytes, &output, numeric(&bytes, b"t") + 8, 4.25);
    let first = bytes
        .windows(8)
        .position(|v| v == 101.0f64.to_be_bytes())
        .unwrap();
    assert_eq!(&output[first..first + 24], &bytes[first..first + 24]);
}

#[test]
fn session_materials_in_a_sourced_scene_are_invalid_and_never_encode() {
    let (_, candidate) = fixture();
    assert_eq!(
        encode(&candidate).unwrap().len(),
        candidate.retained_source_bytes()
    );
    let shape = candidate.shapes[0].id;
    let mut changed = candidate.clone();
    changed.session_materials.assignments.insert(shape, 0);
    assert!(changed.validate().is_err());
    assert_eq!(encode(&changed).unwrap_err().kind(), ErrorKind::Unsupported);
    changed = candidate.clone();
    changed.session_materials.materials[0].color = [0.2, 0.4, 0.6];
    changed.validate().unwrap();
    assert_eq!(encode(&changed).unwrap_err().kind(), ErrorKind::Unsupported);
}

#[test]
fn refuses_sourcefree_absent_origin_invalid_origin_and_structural_changes() {
    assert_eq!(
        encode(&NativeScene::default()).unwrap_err().kind(),
        ErrorKind::Unsupported
    );
    let (_, candidate) = fixture();
    let mut changed = candidate.clone();
    changed.nodes[0].name.push_str("changed");
    assert_eq!(encode(&changed).unwrap_err().kind(), ErrorKind::Unsupported);
    changed = candidate.clone();
    changed.nodes[0].channels[6].value = 2.0;
    changed.nodes[0].channels[6].replaced = true;
    assert!(encode(&changed).is_err());
    changed = candidate.clone();
    changed.nodes[0].channels[0].origin = Some(super::super::super::source::ScalarOrigin::Binary {
        leaf: usize::MAX,
        component: 0,
    });
    assert!(encode(&changed).is_err());
    changed = candidate.clone();
    changed.dag.clear();
    assert!(encode(&changed).is_err());
    let mut transform = NativeTransform::default();
    transform.scale[0] = 2.0;
    assert!(
        candidate
            .clone()
            .set_transform(candidate.nodes()[0].id(), transform)
            .is_err()
    );
}

#[test]
fn retained_reparse_buffers_have_an_exact_boundary_and_checked_overflow() {
    let (bytes, _) = fixture();
    assert_eq!(
        super::super::decode_with_retained_bytes(bytes.clone(), 128 * 1024 * 1024 - bytes.len(), 0)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    let mut low = 0usize;
    let mut high = 128 * 1024 * 1024;
    while high - low > 1 {
        let mid = low + (high - low) / 2;
        if super::super::decode_with_retained_bytes(bytes.clone(), mid, 0).is_ok() {
            low = mid;
        } else {
            high = mid;
        }
    }
    assert!(super::super::decode_with_retained_bytes(bytes.clone(), low, 0).is_ok());
    assert_eq!(
        super::super::decode_with_retained_bytes(bytes.clone(), high, 0)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
    assert_eq!(
        super::super::decode_with_retained_bytes(bytes, usize::MAX, 0)
            .unwrap_err()
            .kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
#[ignore = "requires explicit private manifest and separate SHA precheck"]
fn private_originals_encode_identically_and_edits_change_only_one_word() {
    let manifest = std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST").unwrap();
    let manifest = std::path::Path::new(&manifest);
    let directory = manifest.parent().unwrap().canonicalize().unwrap();
    let entries: serde_json::Value =
        serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 5);
    for entry in entries.as_array().unwrap() {
        let path = directory
            .join(entry["relative_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(path.starts_with(&directory));
        let bytes: Arc<[u8]> = std::fs::read(path).unwrap().into();
        assert_eq!(bytes.len(), entry["bytes"].as_u64().unwrap() as usize);
        let mut candidate = super::super::decode(bytes.clone()).unwrap();
        assert!(
            encode(&candidate).unwrap().as_slice() == bytes.as_ref(),
            "unedited private output differs"
        );
        let source = candidate.source.as_ref().unwrap();
        let leaf = source
            .leaves
            .iter()
            .find(|leaf| leaf.field == super::super::super::TransformField::Translation)
            .unwrap();
        let offset = leaf.numeric.start;
        let node = candidate
            .nodes
            .iter()
            .find(|node| node.transform_id == leaf.owner)
            .unwrap();
        let id = node.id();
        let value = node.transform().translation[0] + 0.125;
        candidate
            .set_channel(id, Channel::TranslateX, value)
            .unwrap();
        let output = encode(&candidate).unwrap();
        assert_only_word(&bytes, &output, offset, value);
        let reopened = super::super::decode(output.into()).unwrap();
        assert_eq!(
            reopened.node(id).unwrap().transform().translation[0].to_bits(),
            value.to_bits()
        );
        assert_eq!(reopened.shapes, candidate.shapes);
        assert_eq!(reopened.dag, candidate.dag);
        assert_eq!(reopened.units, candidate.units);
        assert_eq!(
            ViewScene::from_native(&reopened, 128 * 1024 * 1024)
                .unwrap()
                .instances()
                .len(),
            ViewScene::from_native(&candidate, 128 * 1024 * 1024)
                .unwrap()
                .instances()
                .len()
        );
    }
}

#[test]
fn nodes_differing_only_in_extras_are_structurally_unequal() {
    let (_, scene) = fixture();
    let a = scene.nodes[0].clone();
    let mut b = a.clone();
    assert!(same_node_structure(&a, &b));
    b.extras.shear = Some([0.5, 0.0, 0.0]);
    assert!(!same_node_structure(&a, &b));
}

mod private_differential;

mod hand_built_plans {
    use super::super::insert::Insertion;
    use super::super::{ErrorKind, encode, encode_with};
    use crate::{
        maya_binary::container,
        native_scene::{
            NativeScene,
            binary::{
                decode,
                tests::{
                    site_fixture::{SITE_AUX, other_node_with_triple, sited_transform, triple_aux},
                    wire_fixture::{chunk, group, scene},
                },
            },
            source::TransformField,
        },
    };

    fn record(values: [f64; 3]) -> Vec<u8> {
        triple_aux(b"t", SITE_AUX, values)
    }

    fn children() -> Vec<u8> {
        [
            sited_transform(b"a", &[triple_aux(b"r", SITE_AUX, [1.0; 3])]),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat()
    }

    /// The decoded scene and a plan that inserts `bytes` before the first `DBL3` of the transform.
    fn setup(bytes: Vec<u8>) -> (Vec<u8>, NativeScene, Vec<Insertion>) {
        let source = scene(&children());
        let decoded = decode(source.clone()).unwrap();
        let inspected = container::inspect(source.clone()).unwrap();
        let xfrm = inspected
            .chunks()
            .iter()
            .position(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        let triple = inspected
            .chunks()
            .iter()
            .find(|c| c.parent() == Some(xfrm) && c.tag() == *b"DBL3")
            .unwrap();
        let plan = vec![Insertion {
            owner: decoded.nodes()[0].transform_id,
            field: TransformField::Translation,
            at: triple.header_range().start,
            bytes,
            ancestors: vec![inspected.chunks()[xfrm].header_range().start, 0],
        }];
        (source.to_vec(), decoded, plan)
    }

    #[test]
    fn an_empty_plan_is_todays_encode() {
        let (source, decoded, _) = setup(record([0.0; 3]));
        assert_eq!(encode_with(&decoded, &[], &[]).unwrap(), source);
        assert_eq!(encode(&decoded).unwrap(), source);
    }

    #[test]
    fn a_planned_record_matching_the_scene_is_spliced_and_reparsed() {
        let (source, decoded, plan) = setup(record([0.0; 3]));
        let output = encode_with(&decoded, &plan, &[]).unwrap();
        assert_eq!(output.len(), source.len() + 48);
        let again = decode(output.into()).unwrap();
        assert_eq!(again.nodes()[0].transform().translation, [0.0; 3]);
        let found = again.source.as_ref().unwrap();
        assert!(
            found
                .leaves
                .iter()
                .any(|leaf| leaf.field == TransformField::Translation)
        );
    }

    #[test]
    fn a_planned_record_that_disagrees_with_the_scene_is_refused() {
        for values in [[5.0, 0.0, 0.0], [-0.0, 0.0, 0.0], [0.0, 0.0, 1e-300]] {
            let (_, decoded, plan) = setup(record(values));
            let error = encode_with(&decoded, &plan, &[]).unwrap_err();
            assert_eq!(error.kind(), ErrorKind::Unsupported, "{values:?}");
        }
    }

    #[test]
    fn a_boundary_valid_but_misordered_position_is_refused_by_the_placement_check() {
        let (_, decoded, mut plan) = setup(record([0.0; 3]));
        // After the transform's only triple: a valid container, but `t` must come first.
        let source = scene(&children());
        let inspected = container::inspect(source).unwrap();
        let group = inspected
            .chunks()
            .iter()
            .find(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        assert!(encode_with(&decoded, &plan, &[]).is_ok());
        plan[0].at = group.payload_range().end;
        let error = encode_with(&decoded, &plan, &[]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
    }

    #[test]
    fn a_plan_with_a_missing_ancestor_never_yields_bytes() {
        let (_, decoded, plan) = setup(record([0.0; 3]));
        // Without the root the grown group overruns it and the reparse cannot read the file.
        let mut unrooted = plan_with_ancestors(&plan, |a| a.pop().map(drop).unwrap_or(()));
        assert_eq!(
            encode_with(&decoded, &unrooted, &[]).unwrap_err().kind(),
            ErrorKind::Malformed
        );
        // Root-parent layout: without the innermost group the record would be the group's sibling,
        // but the root pads its children to 4, so the container is truncated. The nested layout in
        // `place::tests::nested` is where only the placement check refuses it.
        unrooted = plan_with_ancestors(&plan, |a| {
            a.remove(0);
        });
        let source = scene(&children());
        let inspected = container::inspect(source).unwrap();
        let group = inspected
            .chunks()
            .iter()
            .find(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        unrooted[0].at = group.payload_range().end;
        assert_eq!(
            encode_with(&decoded, &unrooted, &[]).unwrap_err().kind(),
            ErrorKind::Malformed
        );
    }

    fn plan_with_ancestors(
        plan: &[Insertion],
        edit: impl FnOnce(&mut Vec<usize>),
    ) -> Vec<Insertion> {
        let mut ancestors = plan[0].ancestors.clone();
        edit(&mut ancestors);
        vec![Insertion {
            owner: plan[0].owner,
            field: plan[0].field,
            at: plan[0].at,
            bytes: plan[0].bytes.clone(),
            ancestors,
        }]
    }

    #[test]
    fn a_splice_past_the_container_limit_is_refused_not_emitted() {
        let head = |info: usize| {
            let header = [
                chunk(b"LUNI", b"cm", 8),
                chunk(b"AUNI", b"deg", 8),
                chunk(b"TUNI", b"film", 8),
                chunk(b"INFO", &vec![b'x'; info], 8),
            ]
            .concat();
            group(
                b"Maya",
                &[group(b"HEAD", &header, 4), children()].concat(),
                1,
            )
        };
        let mut info = container::MAX_BYTES - 10_000;
        let base = head(info).len();
        info += (container::MAX_BYTES - 47 - base).next_multiple_of(8);
        let source = head(info);
        assert!((container::MAX_BYTES - 47..=container::MAX_BYTES).contains(&source.len()));
        let source: std::sync::Arc<[u8]> = source.into();
        let decoded = match decode(source.clone()) {
            Ok(decoded) => decoded,
            Err(error) => {
                // A false resource refusal near the limit is acceptable; an overshoot is not.
                assert_eq!(error.kind(), ErrorKind::ResourceLimit);
                return;
            }
        };
        let inspected = container::inspect(source).unwrap();
        let xfrm = inspected
            .chunks()
            .iter()
            .position(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        let triple = inspected
            .chunks()
            .iter()
            .find(|c| c.parent() == Some(xfrm) && c.tag() == *b"DBL3")
            .unwrap();
        let plan = [Insertion {
            owner: decoded.nodes()[0].transform_id,
            field: TransformField::Translation,
            at: triple.header_range().start,
            bytes: record([0.0; 3]),
            ancestors: vec![inspected.chunks()[xfrm].header_range().start, 0],
        }];
        let error = encode_with(&decoded, &plan, &[]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::ResourceLimit);
    }

    #[test]
    fn appearance_located_after_the_insertion_survives_the_shift() {
        let source: std::sync::Arc<[u8]> =
            crate::native_scene::binary::tests::appearance_fixture::appearance_specimen().into();
        let decoded = decode(source.clone()).unwrap();
        assert!(!decoded.materials.is_empty());
        let inspected = container::inspect(source.clone()).unwrap();
        let xfrm = inspected
            .chunks()
            .iter()
            .position(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        let group = &inspected.chunks()[xfrm];
        let plan = [Insertion {
            owner: decoded.nodes()[0].transform_id,
            field: TransformField::Rotation,
            at: group.payload_range().end,
            // The specimen's own aux word: a record with any other word is refused.
            bytes: triple_aux(b"r", 0x1234_5678, [0.0; 3]),
            ancestors: vec![group.header_range().start, 0],
        }];
        let output = encode_with(&decoded, &plan, &[]).unwrap();
        assert_eq!(output.len(), source.len() + 48);
        let again = decode(output.into()).unwrap();
        assert!(again.materials == decoded.materials);
        assert_eq!(again.nodes()[0].transform().translation, [1.0, 2.0, 3.0]);
    }
}

mod enabled {
    use super::super::encode;
    use crate::native_scene::{
        Channel, NativeError, NativeScene, NativeTransform,
        binary::{
            decode,
            tests::{
                site_fixture::{
                    SITE_AUX, double_aux, other_node_with_triple, sited_transform, triple_aux,
                },
                wire_fixture::{scene, scene_with_units},
            },
        },
    };
    use std::sync::Arc;

    fn t(values: [f64; 3]) -> Vec<u8> {
        triple_aux(b"t", SITE_AUX, values)
    }

    /// `a` has no translation; `b` has one; `c` has none and no site (`v` after a triple).
    fn children(a: &[Vec<u8>]) -> Vec<u8> {
        [
            sited_transform(b"a", a),
            sited_transform(
                b"b",
                &[t([7.0, 8.0, 9.0]), triple_aux(b"r", SITE_AUX, [1.0; 3])],
            ),
            sited_transform(
                b"c",
                &[
                    triple_aux(b"r", SITE_AUX, [2.0; 3]),
                    double_aux(b"v", 0x24, SITE_AUX, 1.0),
                ],
            ),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat()
    }

    fn open() -> (Arc<[u8]>, NativeScene) {
        let bytes = scene(&children(&[]));
        (bytes.clone(), decode(bytes).unwrap())
    }

    fn translation(scene: &NativeScene, node: usize) -> [u64; 3] {
        scene.nodes()[node]
            .transform()
            .translation
            .map(f64::to_bits)
    }

    #[test]
    fn an_edited_absent_translation_is_inserted_exactly_where_maya_writes_it() {
        let (_, mut edited) = open();
        let id = edited.nodes()[0].id();
        edited.set_channel(id, Channel::TranslateX, 5.0).unwrap();
        let output = encode(&edited).unwrap();
        let expected = scene(&children(&[t([5.0, 0.0, 0.0])]));
        assert_eq!(output, expected.as_ref());
        let reopened = decode(output.clone().into()).unwrap();
        assert_eq!(
            translation(&reopened, 0),
            [5.0f64.to_bits(), 0.0f64.to_bits(), 0.0f64.to_bits()]
        );
        let (_, untouched) = open();
        for node in 1..3 {
            assert_eq!(translation(&reopened, node), translation(&untouched, node));
        }
        assert_eq!(reopened.shapes, untouched.shapes);
        assert_eq!(encode(&edited).unwrap(), output, "saving twice is stable");
    }

    #[test]
    fn components_and_signed_zero_land_in_their_own_words() {
        for (channel, value, words) in [
            (Channel::TranslateY, 2.5, [0.0, 2.5, 0.0]),
            (Channel::TranslateZ, -0.0, [0.0, 0.0, -0.0]),
        ] {
            let (_, mut edited) = open();
            let id = edited.nodes()[0].id();
            edited.set_channel(id, channel, value).unwrap();
            let expected = scene(&children(&[t(words)]));
            assert_eq!(encode(&edited).unwrap(), expected.as_ref(), "{channel:?}");
        }
    }

    #[test]
    fn declared_units_are_stored_in_centimetres() {
        let bytes = scene_with_units(&children(&[]), b"m", b"deg");
        let mut edited = decode(bytes).unwrap();
        let id = edited.nodes()[0].id();
        edited.set_channel(id, Channel::TranslateX, 2.0).unwrap();
        let expected = scene_with_units(&children(&[t([200.0, 0.0, 0.0])]), b"m", b"deg");
        assert_eq!(encode(&edited).unwrap(), expected.as_ref());
    }

    #[test]
    fn editing_back_to_the_default_saves_the_input_byte_for_byte() {
        let (bytes, mut edited) = open();
        let id = edited.nodes()[0].id();
        edited.set_channel(id, Channel::TranslateX, 5.0).unwrap();
        edited.set_channel(id, Channel::TranslateX, 0.0).unwrap();
        assert_eq!(encode(&edited).unwrap(), bytes.as_ref());
    }

    #[test]
    fn nodes_without_a_site_and_ungated_fields_keep_their_refusal() {
        let (_, mut edited) = open();
        let siteless = edited.nodes()[2].id();
        assert_eq!(
            edited.set_channel(siteless, Channel::TranslateX, 5.0),
            Err(NativeError::UnsupportedEdit)
        );
        let sited = edited.nodes()[0].id();
        for channel in [Channel::RotateX, Channel::ScaleX] {
            assert_eq!(
                edited.set_channel(sited, channel, 2.0),
                Err(NativeError::UnsupportedEdit),
                "{channel:?}"
            );
        }
        let mut noisy = edited.nodes()[0].transform();
        noisy.rotation[0] = 1e-12;
        assert_eq!(
            edited.set_transform(sited, noisy),
            Err(NativeError::UnsupportedEdit)
        );
        let same: NativeTransform = edited.nodes()[0].transform();
        assert!(edited.set_transform(sited, same).is_ok());
    }

    #[test]
    fn a_signed_zero_companion_of_a_real_edit_reopens_bit_equal() {
        let (_, mut edited) = open();
        let id = edited.nodes()[0].id();
        edited.set_channel(id, Channel::TranslateX, 5.0).unwrap();
        edited.set_channel(id, Channel::TranslateY, -0.0).unwrap();
        let output = encode(&edited).unwrap();
        assert_eq!(output, scene(&children(&[t([5.0, -0.0, 0.0])])).as_ref());
        let reopened = decode(output.into()).unwrap();
        assert_eq!(
            translation(&reopened, 0),
            [5.0f64.to_bits(), (-0.0f64).to_bits(), 0.0f64.to_bits()]
        );
    }

    #[test]
    fn float_noise_on_an_absent_rotation_or_scale_still_refuses_through_set_transform() {
        // The rotation case is also pinned in `nodes_without_a_site_and_ungated_fields_keep_…`.
        let (_, mut edited) = open();
        let id = edited.nodes()[0].id();
        let mut noisy = edited.nodes()[0].transform();
        noisy.scale[0] = 1.0 + 1e-12;
        assert_eq!(
            edited.set_transform(id, noisy),
            Err(NativeError::UnsupportedEdit)
        );
        let mut noisy = edited.nodes()[0].transform();
        noisy.translation[0] = 5.0;
        noisy.rotation[2] = 1e-12;
        assert_eq!(
            edited.set_transform(id, noisy),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(translation(&edited, 0), [0.0f64.to_bits(); 3]);
    }

    /// The appearance specimen with Maya-style zero padding and its object's `t` renamed `r`, so
    /// the object has no translation but a site, and its shape and material follow it.
    fn appearance_without_translation() -> Arc<[u8]> {
        let mut bytes =
            crate::native_scene::binary::tests::appearance_fixture::appearance_specimen();
        let tree = crate::maya_binary::container::inspect(bytes.clone().into()).unwrap();
        for chunk in tree.chunks() {
            bytes[chunk.padding_range()].fill(0);
        }
        let translation = tree
            .chunks()
            .iter()
            .find(|c| c.tag() == *b"DBL3" && bytes[c.payload_range()].starts_with(b"t\0"))
            .unwrap();
        bytes[translation.payload_range().start] = b'r';
        bytes.into()
    }

    /// Removes the 48 inserted bytes and requires the rest to equal `input` except for the two size
    /// words of the enclosing groups (+48) and the one diffuse word (0.8 became 0.25).
    fn assert_only_expected_bytes_differ(input: &[u8], output: &[u8]) {
        let at = output
            .windows(19)
            .position(|w| &w[..4] == b"DBL3" && &w[16..19] == b"t\0\x20")
            .unwrap();
        let stripped = [&output[..at], &output[at + 48..]].concat();
        assert_eq!(stripped.len(), input.len());
        let group = input.windows(4).position(|w| w == b"XFRM").unwrap() - 16;
        let diffuse = input
            .windows(8)
            .position(|w| w == 0.8f64.to_be_bytes())
            .unwrap();
        let allowed = [8..16, group + 8..group + 16, diffuse..diffuse + 8];
        for (index, (a, b)) in input.iter().zip(&stripped).enumerate() {
            assert!(
                a == b || allowed.iter().any(|range| range.contains(&index)),
                "byte {index} changed outside the expected words"
            );
        }
        let word =
            |bytes: &[u8], at: usize| u64::from_be_bytes(bytes[at..at + 8].try_into().unwrap());
        for size in [8, group + 8] {
            assert_eq!(word(&stripped, size), word(input, size) + 48);
        }
        assert_eq!(stripped[diffuse..diffuse + 8], 0.25f64.to_be_bytes());
    }

    #[test]
    fn a_shape_appearance_after_the_inserted_leaf_keeps_its_diffuse_edit_in_the_same_save() {
        let mut edited = decode(appearance_without_translation()).unwrap();
        let object = edited.nodes()[0].id();
        let shape = edited.shapes()[0].id();
        assert!(!edited.source.as_ref().unwrap().sites.is_empty());
        let before = edited.material_for_shape(shape).unwrap();
        edited
            .set_channel(object, Channel::TranslateX, 4.0)
            .unwrap();
        edited.set_material_diffuse(shape, 0.25).unwrap();
        let output = encode(&edited).unwrap();
        let input = edited.source.as_ref().unwrap().bytes.to_vec();
        assert_eq!(output.len(), input.len() + 48);
        assert_only_expected_bytes_differ(&input, &output);
        let reopened = decode(output.into()).unwrap();
        assert_eq!(translation(&reopened, 0)[0], 4.0f64.to_bits());
        let after = reopened.material_for_shape(shape).unwrap();
        assert_eq!(after.diffuse.to_bits(), 0.25f64.to_bits());
        assert_ne!(before.diffuse.to_bits(), after.diffuse.to_bits());
        assert_eq!(after.shared_users, before.shared_users);
    }

    mod moves {
        use super::*;
        use crate::{
            history::{
                EditOutcome, History, ManipulationError, ManipulationFinish, ManipulationProvenance,
            },
            native_scene::{ChannelLeaf, TransformTarget, WorldAxis},
        };
        use uuid::Uuid;

        const BUDGET: usize = 256 * 1024 * 1024;

        fn target(history: &History, node: usize) -> TransformTarget {
            let id = history.scene().nodes()[node].id();
            history
                .scene()
                .transform_paths()
                .unwrap()
                .into_iter()
                .find(|path| path.node == id)
                .unwrap()
        }

        fn move_x(history: &mut History, node: usize, distance: f64) {
            let token = Uuid::new_v4();
            let target = target(history, node);
            let id = history
                .begin_move(
                    &target,
                    WorldAxis::X,
                    token,
                    ManipulationProvenance::Native,
                    BUDGET,
                )
                .unwrap();
            let native = ManipulationProvenance::Native;
            history.update_move(token, id, native, distance).unwrap();
            history
                .finish_move(token, id, native, ManipulationFinish::Commit)
                .unwrap();
        }

        #[test]
        fn native_move_of_an_identity_node_saves_and_undo_redo_restore_the_absent_leaf() {
            let (bytes, scene) = open();
            let mut history = History::new(scene);
            let original = history.scene().clone();
            move_x(&mut history, 0, 3.0);
            let expected = self::scene(&children(&[t([3.0, 0.0, 0.0])]));
            assert_eq!(encode(history.scene()).unwrap(), expected.as_ref());
            let moved = history.scene().clone();

            assert!(history.undo());
            assert!(*history.scene() == original);
            let leaf = &history.scene().nodes[0].channels[0];
            assert!(leaf.origin.is_none() && !leaf.replaced);
            assert_eq!(encode(history.scene()).unwrap(), bytes.as_ref());

            assert!(history.redo());
            assert!(*history.scene() == moved);
            assert_eq!(encode(history.scene()).unwrap(), expected.as_ref());
        }

        #[test]
        fn rotate_and_scale_keep_the_absent_leaf_refusal_on_a_sited_node() {
            use crate::native_scene::{RotateAxis, RotateSpace, ScaleHandle, ScaleSpace};
            let (_, scene) = open();
            let mut history = History::new(scene);
            let target = target(&history, 0);
            let native = ManipulationProvenance::Native;
            let rotate = history
                .begin_rotate(
                    &target,
                    RotateAxis::X,
                    RotateSpace::Object,
                    Uuid::new_v4(),
                    native,
                    BUDGET,
                )
                .unwrap_err();
            assert!(matches!(
                rotate,
                ManipulationError::Native(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Rotate,
                    component: 0
                })
            ));
            let scale = history
                .begin_scale(
                    &target,
                    ScaleHandle::X,
                    ScaleSpace::Object,
                    Uuid::new_v4(),
                    native,
                    BUDGET,
                )
                .unwrap_err();
            assert!(matches!(
                scale,
                ManipulationError::Native(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Scale,
                    component: 0
                })
            ));
            assert!(history.active_manipulation().is_none());
        }

        #[test]
        fn cancel_restores_an_unreplaced_origin_less_leaf_and_a_byte_identical_save() {
            let (bytes, scene) = open();
            let mut history = History::new(scene);
            let original = history.scene().clone();
            let (token, native) = (Uuid::new_v4(), ManipulationProvenance::Native);
            let target = target(&history, 0);
            let id = history
                .begin_move(&target, WorldAxis::X, token, native, BUDGET)
                .unwrap();
            history.update_move(token, id, native, 3.0).unwrap();
            assert!(history.scene().nodes[0].channels[0].replaced);
            history
                .finish_move(token, id, native, ManipulationFinish::Cancel)
                .unwrap();
            assert!(*history.scene() == original);
            let leaf = &history.scene().nodes[0].channels[0];
            assert!(leaf.origin.is_none() && !leaf.replaced);
            assert!(!history.undo());
            assert_eq!(encode(history.scene()).unwrap(), bytes.as_ref());
        }

        #[test]
        fn moving_out_and_back_to_zero_commits_nothing() {
            let (bytes, scene) = open();
            let mut history = History::new(scene);
            let original = history.scene().clone();
            let (token, native) = (Uuid::new_v4(), ManipulationProvenance::Native);
            let target = target(&history, 0);
            let id = history
                .begin_move(&target, WorldAxis::X, token, native, BUDGET)
                .unwrap();
            history.update_move(token, id, native, 3.0).unwrap();
            history.update_move(token, id, native, 0.0).unwrap();
            let outcome = history
                .finish_move(token, id, native, ManipulationFinish::Commit)
                .unwrap();
            assert_eq!(outcome, EditOutcome::Unchanged);
            let leaf = &history.scene().nodes[0].channels[0];
            assert!(leaf.origin.is_none() && !leaf.replaced);
            assert!(*history.scene() == original);
            assert!(!history.undo());
            assert_eq!(encode(history.scene()).unwrap(), bytes.as_ref());
        }

        #[test]
        fn a_node_without_a_site_keeps_the_absent_leaf_refusal() {
            let (_, scene) = open();
            let mut history = History::new(scene);
            let target = target(&history, 2);
            let error = history
                .begin_move(
                    &target,
                    WorldAxis::X,
                    Uuid::new_v4(),
                    ManipulationProvenance::Native,
                    BUDGET,
                )
                .unwrap_err();
            assert!(matches!(
                error,
                ManipulationError::Native(NativeError::AbsentChannelLeaf {
                    channel: ChannelLeaf::Translate,
                    component: 0
                })
            ));
            assert!(history.active_manipulation().is_none());
        }
    }

    /// Two sited nodes without a translation (`a`, `d`) and one with it.
    fn pair(a: &[Vec<u8>], d: &[Vec<u8>]) -> Vec<u8> {
        [
            sited_transform(b"a", a),
            sited_transform(b"d", d),
            sited_transform(b"b", &[t([7.0, 8.0, 9.0])]),
            other_node_with_triple(b"anchor", SITE_AUX),
        ]
        .concat()
    }

    #[test]
    fn an_elided_leaf_and_an_insertion_in_one_save() {
        let mut edited = decode(scene(&pair(&[], &[]))).unwrap();
        let (a, d) = (edited.nodes()[0].id(), edited.nodes()[1].id());
        edited.set_channel(d, Channel::TranslateX, 5.0).unwrap();
        edited.set_channel(d, Channel::TranslateX, 0.0).unwrap();
        edited.set_channel(a, Channel::TranslateY, 3.0).unwrap();
        let expected = scene(&pair(&[t([0.0, 3.0, 0.0])], &[]));
        assert_eq!(encode(&edited).unwrap(), expected.as_ref());
    }

    #[test]
    fn owner_lookups_preserve_mixed_edits_in_non_alphabetical_file_order() {
        let names: [&[u8]; 6] = [b"z", b"m", b"y", b"b", b"a", b"x"];
        let rotation = triple_aux(b"r", SITE_AUX, [1.0; 3]);
        let children: Vec<_> = names
            .iter()
            .flat_map(|name| sited_transform(name, std::slice::from_ref(&rotation)))
            .collect();
        let mut edited = decode(scene(&children)).unwrap();
        let mut expected = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let id = edited.nodes()[index].id();
            let mut records = Vec::new();
            match index % 3 {
                0 => {
                    edited.set_channel(id, Channel::TranslateY, 3.0).unwrap();
                    records.push(t([0.0, 3.0, 0.0]));
                }
                1 => {
                    edited.set_channel(id, Channel::TranslateX, 5.0).unwrap();
                    edited.set_channel(id, Channel::TranslateX, 0.0).unwrap();
                }
                _ => {}
            }
            if index % 3 != 2 {
                edited.set_channel(id, Channel::RotateY, 2.0).unwrap();
            }
            records.push(triple_aux(
                b"r",
                SITE_AUX,
                [
                    1.0,
                    if index % 3 == 2 {
                        1.0
                    } else {
                        2.0f64.to_radians()
                    },
                    1.0,
                ],
            ));
            expected.extend(sited_transform(name, &records));
        }
        assert_eq!(encode(&edited).unwrap(), scene(&expected).as_ref());
    }

    #[test]
    fn has_field_distinguishes_fields_of_one_owner() {
        use super::super::has_field;
        use crate::native_scene::source::TransformField::{Rotation, Scale, Translation};
        let edited = decode(scene(&pair(&[], &[]))).unwrap();
        let (a, d) = (edited.nodes[0].transform_id, edited.nodes[1].transform_id);
        assert!(a < d);
        let fields = [
            (a, Translation),
            (a, Scale),
            (d, Rotation),
            (d, Translation),
        ];
        // Several fields per owner, so a `take(1)` lookup would miss the second one.
        assert!(fields.windows(2).any(|w| w[0].0 == w[1].0));
        for (owner, field) in fields {
            assert!(has_field(&fields, owner, field));
        }
        assert!(!has_field(&fields, a, Rotation));
        assert!(!has_field(&fields, d, Scale));
        assert!(!has_field(&fields[1..], a, Translation));
        assert!(!has_field(&[], a, Translation));
    }

    #[test]
    fn has_field_with_one_field_per_owner_matches_only_that_field() {
        use super::super::has_field;
        use crate::native_scene::source::TransformField::{Rotation, Scale, Translation};
        let edited = decode(scene(&pair(&[], &[]))).unwrap();
        let (a, d) = (edited.nodes[0].transform_id, edited.nodes[1].transform_id);
        let fields = [(a, Translation), (d, Rotation)];
        assert!(has_field(&fields, a, Translation));
        assert!(has_field(&fields, d, Rotation));
        for (owner, field) in [(a, Rotation), (a, Scale), (d, Translation), (d, Scale)] {
            assert!(!has_field(&fields, owner, field));
        }
        assert!(!has_field(&fields[..1], d, Rotation));
        assert!(!has_field(&fields[1..], a, Translation));
    }

    #[test]
    fn an_unsorted_elided_list_keeps_the_lookups_exact() {
        use super::super::{encode_with, plan::plan};
        use crate::native_scene::source::{INSERTABLE, SourceFormat};
        let bytes = scene(&pair(&[], &[]));
        let mut edited = decode(bytes.clone()).unwrap();
        for index in 0..2 {
            let id = edited.nodes()[index].id();
            edited.set_channel(id, Channel::TranslateX, 5.0).unwrap();
            edited.set_channel(id, Channel::TranslateX, 0.0).unwrap();
        }
        let source = edited
            .source
            .as_ref()
            .filter(|source| source.format == SourceFormat::Binary)
            .unwrap();
        let planned = plan(&edited, source, INSERTABLE).unwrap();
        assert!(planned.insertions.is_empty());
        assert_eq!(planned.elided.len(), 2);
        let mut reversed = planned.elided.clone();
        reversed.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
        assert_ne!(planned.elided, reversed);
        assert_eq!(
            encode_with(&edited, &planned.insertions, &reversed).unwrap(),
            bytes.as_ref()
        );
    }

    #[test]
    fn a_value_that_underflows_to_zero_centimetres_is_elided() {
        let bytes = scene_with_units(&children(&[]), b"mm", b"deg");
        let mut edited = decode(bytes.clone()).unwrap();
        let id = edited.nodes()[0].id();
        edited.set_channel(id, Channel::TranslateX, 5e-324).unwrap();
        assert_eq!(edited.nodes()[0].channels[0].value, 5e-324);
        assert!(edited.nodes()[0].channels[0].replaced);
        assert_eq!(encode(&edited).unwrap(), bytes.as_ref());
    }

    #[test]
    fn a_forged_edit_on_a_node_without_a_site_is_refused_at_encode() {
        let (_, mut forged) = open();
        forged.nodes[2].channels[0].value = 2.0;
        forged.nodes[2].channels[0].replaced = true;
        assert!(forged.validate().is_err());
        assert!(encode(&forged).is_err());
    }
}

mod nested_hand_built {
    use super::super::{ErrorKind, encode_with, insert::Insertion};
    use crate::{
        maya_binary::container,
        native_scene::{
            binary::{
                decode,
                tests::{
                    site_fixture::{
                        SITE_AUX, creation_zero_pad, group_zero_pad, other_node_with_triple,
                        triple_aux,
                    },
                    wire_fixture::scene,
                },
            },
            source::TransformField,
        },
    };

    #[test]
    fn a_record_that_would_sit_beside_its_group_is_refused_by_the_placement_check() {
        let inner = group_zero_pad(
            b"XFRM",
            &[
                creation_zero_pad(b"inner", None),
                triple_aux(b"r", SITE_AUX, [1.0; 3]),
            ]
            .concat(),
            8,
        );
        let holder = group_zero_pad(
            b"AMBL",
            &[creation_zero_pad(b"holder", None), inner].concat(),
            4,
        );
        let source = scene(&[holder, other_node_with_triple(b"anchor", SITE_AUX)].concat());
        let decoded = decode(source.clone()).unwrap();
        let tree = container::inspect(source).unwrap();
        let chunks = tree.chunks();
        let inner = chunks
            .iter()
            .position(|c| c.form() == Some(*b"XFRM"))
            .unwrap();
        let holder = chunks[inner].parent().unwrap();
        let plan = |at: usize, ancestors: Vec<usize>| {
            vec![Insertion {
                owner: decoded.nodes()[0].transform_id,
                field: TransformField::Translation,
                at,
                bytes: triple_aux(b"t", SITE_AUX, [0.0; 3]),
                ancestors,
            }]
        };
        let triple = chunks
            .iter()
            .find(|c| c.parent() == Some(inner) && c.tag() == *b"DBL3")
            .unwrap()
            .header_range()
            .start;
        let (inner_at, holder_at) = (
            chunks[inner].header_range().start,
            chunks[holder].header_range().start,
        );
        let proper = plan(triple, vec![inner_at, holder_at, 0]);
        assert!(encode_with(&decoded, &proper, &[]).is_ok());
        let beside = plan(chunks[inner].padding_range().end, vec![holder_at, 0]);
        let error = encode_with(&decoded, &beside, &[]).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
    }
}
