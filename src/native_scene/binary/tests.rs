use super::*;
pub(super) mod wire_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}
pub(super) mod site_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary_sites.rs"
    ));
}
pub(super) mod appearance_fixture {
    use super::wire_fixture as wire;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_appearance.rs"
    ));
}
mod sites;
use crate::view_scene::ViewScene;
use std::{fs::File, io::Read, path::Path};
use wire_fixture::{chunk, creation, group, mesh_payload, scene, triple};

pub(super) fn read(path: &Path) -> Vec<u8> {
    let mut data = Vec::new();
    File::open(path)
        .expect("explicit private input must exist")
        .take(container::MAX_BYTES as u64 + 1)
        .read_to_end(&mut data)
        .unwrap();
    assert!(data.len() <= container::MAX_BYTES);
    data
}

#[test]
fn shared_protocol_specimen_uses_the_actual_checked_decoder() {
    let scene = decode(wire_fixture::specimen().into()).unwrap();
    assert_eq!(scene.nodes()[0].transform().translation[0], 16777217.125);
    assert_eq!(
        scene
            .internal_transform(scene.nodes()[0].id())
            .unwrap()
            .rotation_radians[2],
        0.25
    );
    assert_eq!(
        ViewScene::from_native(&scene, 1024 * 1024)
            .unwrap()
            .instances()
            .len(),
        1
    );
}

/// Reading joints is out of scope (#363), so a joint group must fail the Open, never leave the
/// scene without its joints. Maya's real form tag is unverified; `JOIN` is the one observed in
/// retained vendor streams. The decoder has no joint case, so any unlisted form must refuse.
#[test]
fn a_joint_form_is_refused_not_dropped_from_the_scene() {
    let mut bytes = wire_fixture::specimen();
    let offset = bytes.len();
    bytes.extend(group(b"JOIN", &creation(b"joint1", Some(b"object")), 4));
    let size = (bytes.len() - 16) as u64;
    bytes[8..16].copy_from_slice(&size.to_be_bytes());
    let source: Arc<[u8]> = bytes.into();
    let refusal = container::inspect(source.clone()).unwrap_err();
    assert_eq!(
        (refusal.kind(), refusal.offset()),
        (container::ErrorKind::Unsupported, offset)
    );
    assert!(decode(source).is_err());
}

#[test]
fn ncrv_is_opaque_to_inspection_but_refused_before_native_construction() {
    let baseline = wire_fixture::specimen();
    let candidate = decode(baseline.clone().into()).unwrap();
    assert_eq!(candidate.nodes().len(), 1);
    assert_eq!(candidate.shapes().len(), 1);
    for children in [
        Vec::new(),
        [
            creation(b"curve", Some(b"object")),
            chunk(b"NRBC", &[0xff, 0, 7], 8),
        ]
        .concat(),
    ] {
        let mut bytes = baseline.clone();
        let offset = bytes.len();
        bytes.extend(group(b"NCRV", &children, 4));
        let size = (bytes.len() - 16) as u64;
        bytes[8..16].copy_from_slice(&size.to_be_bytes());
        let source: Arc<[u8]> = bytes.into();
        let inspected = container::inspect(source.clone()).unwrap();
        assert!(Arc::ptr_eq(inspected.source(), &source));
        if !children.is_empty() {
            let opaque = inspected
                .chunks()
                .iter()
                .position(|chunk| chunk.tag() == *b"NRBC")
                .unwrap();
            assert_eq!(inspected.payload(opaque), Some([0xff, 0, 7].as_slice()));
        }
        let error = decode(source).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported);
        assert_eq!(error.offset(), Some(offset));
    }
}

#[test]
fn effective_last_origin_and_signed_zero_retain_earlier_source() {
    let earlier = triple(b"t", [12.0, 13.0, 14.0]);
    let last = triple(b"t", [-0.0, 2.0, 3.0]);
    let bytes = scene(&group(
        b"XFRM",
        &[creation(b"object", None), earlier, last].concat(),
        4,
    ));
    let scene = decode(bytes.clone()).unwrap();
    let source = scene.source.as_ref().unwrap();
    assert!(Arc::ptr_eq(&bytes, &source.bytes));
    assert_eq!(source.leaves.len(), 1);
    let node = &scene.nodes[0];
    assert_eq!(
        scene.internal_transform(node.id()).unwrap().translation_cm[0].to_bits(),
        (-0.0f64).to_bits()
    );
    let leaf = &source.leaves[0];
    assert_eq!(
        f64::from_be_bytes(
            bytes[leaf.numeric.start..leaf.numeric.start + 8]
                .try_into()
                .unwrap()
        )
        .to_bits(),
        (-0.0f64).to_bits()
    );
    assert_eq!(
        node.channels[0].origin,
        Some(super::super::source::ScalarOrigin::Binary {
            leaf: 0,
            component: 0
        })
    );
    assert_eq!(source.bytes.as_ref(), bytes.as_ref());
}

#[test]
fn production_mesh_candidate_and_view_share_canonical_geometry() {
    let mesh = chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8);
    let bytes = scene(
        &[
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"DMSH",
                &[creation(b"shape", Some(b"object")), mesh].concat(),
                4,
            ),
        ]
        .concat(),
    );
    let candidate = decode(bytes).unwrap();
    assert_eq!(candidate.shapes()[0].mesh().point_count(), 3);
    assert_eq!(candidate.shapes()[0].mesh().edges()[0].smooth(), None);
    let view = ViewScene::from_native(&candidate, 1024 * 1024).unwrap();
    assert_eq!(view.instances().len(), 1);
}

#[test]
fn source_words_origins_units_parent_and_pivots_reach_world_pose() {
    let angle = 0.37f64;
    let values = [
        [3.0, 4.0, 5.0],
        [0.0, -0.0, angle],
        [2.0, 1.0, 1.0],
        [2.0, 3.0, 0.0],
        [1.0, 1.0, 0.0],
    ];
    let mut child = creation(b"child", Some(b"root"));
    for (key, value) in [b"t".as_slice(), b"r", b"s", b"rp", b"sp"]
        .iter()
        .zip(values)
    {
        child.extend(triple(key, value));
    }
    let bytes = wire_fixture::scene_with_units(
        &[
            group(
                b"XFRM",
                &[creation(b"root", None), triple(b"t", [10.0, -5.0, 1.0])].concat(),
                4,
            ),
            group(b"XFRM", &child, 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"child")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
        b"m",
        b"rad",
    );
    let candidate = decode(bytes.clone()).unwrap();
    assert_eq!(candidate.units().linear, super::super::LinearUnit::Meter);
    assert_eq!(candidate.units().angular, super::super::AngularUnit::Radian);
    let root = &candidate.nodes[0];
    let child = &candidate.nodes[1];
    assert_eq!(child.transform().translation, [0.03, 0.04, 0.05]);
    let actual = candidate.internal_transform(child.id()).unwrap();
    for (actual, expected) in [
        actual.translation_cm,
        actual.rotation_radians,
        actual.scale,
        actual.rotate_pivot_cm,
        actual.scale_pivot_cm,
    ]
    .into_iter()
    .zip(values)
    {
        assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
    }
    let source = candidate.source.as_ref().unwrap();
    assert_eq!(source.leaves.len(), 6);
    for leaf in source
        .leaves
        .iter()
        .filter(|leaf| leaf.owner == child.transform_id())
    {
        use super::super::source::TransformField::*;
        let index = [Translation, Rotation, Scale, RotatePivot, ScalePivot]
            .iter()
            .position(|field| *field == leaf.field)
            .unwrap();
        let words = bytes[leaf.numeric.clone()].as_chunks::<8>().0;
        assert_eq!(
            words
                .iter()
                .map(|w| f64::from_be_bytes(*w).to_bits())
                .collect::<Vec<_>>(),
            values[index].map(f64::to_bits)
        );
    }
    let view = ViewScene::from_native(&candidate, 1024 * 1024).unwrap();
    assert_eq!(
        view.instances()[0].path(),
        [
            root.transform_id(),
            child.transform_id(),
            candidate.shapes()[0].id()
        ]
    );
    let (sin, cos) = angle.sin_cos();
    let expected = [
        15.0 - 3.0 * cos + 3.0 * sin,
        2.0 - 3.0 * sin - 3.0 * cos,
        6.0,
    ];
    let actual = view.instances()[0].world_vertices()[0].to_array();
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((f64::from(actual) - expected).abs() < 0.00001);
    }
}

fn attribute(kind: &[u8; 4], name: &[u8]) -> Vec<u8> {
    let flags: &[u8] = match kind {
        b"aLI1" => b"\xf8\x78\xc8",
        b"aENM" => b"\xf8\x02\x4e",
        b"aDBL" => b"\xf8\x00\x4e",
        _ => unreachable!(),
    };
    let mut bytes = kind.to_vec();
    bytes.push(0);
    bytes.extend(flags);
    bytes.extend([0, 0, 0, 1, 0, 0, 0, 0]);
    bytes.extend(name);
    bytes.push(0);
    bytes.extend(name);
    bytes.push(0);
    if kind == b"aENM" {
        bytes.extend(b"low:high\0");
    }
    bytes.extend([0; 4]);
    bytes.extend(0.0f64.to_be_bytes());
    bytes.extend(1.0f64.to_be_bytes());
    bytes.extend(b"DBLE");
    bytes.extend(0.0f64.to_be_bytes());
    chunk(b"ATTR", &bytes, 8)
}

fn state(tag: &[u8; 4], name: &[u8]) -> Vec<u8> {
    let mut bytes = name.to_vec();
    bytes.extend([0, 0x24]);
    if tag == b"DBLE" {
        bytes.extend(0.5f64.to_be_bytes());
    }
    chunk(tag, &bytes, 8)
}

fn tweak(key: &[u8], values: &[[f32; 3]]) -> Vec<u8> {
    let mut bytes = key.to_vec();
    bytes.extend([0, 0x20]);
    for point in values {
        for value in point {
            bytes.extend(value.to_be_bytes());
        }
    }
    chunk(b"FLT3", &bytes, 8)
}

fn hint(count: u32) -> Vec<u8> {
    let mut bytes = b"pt\0\x28".to_vec();
    bytes.extend(count.to_be_bytes());
    chunk(b"FLGS", &bytes, 8)
}

fn cached_mesh() -> Vec<u8> {
    let mut mesh = mesh_payload(&[0, 1, 0x6000_0002]);
    mesh[7..11].copy_from_slice(&16777216.0f32.to_be_bytes());
    mesh[11..15].copy_from_slice(&(-0.0f32).to_be_bytes());
    chunk(b"MESH", &[b"ci".as_slice(), &mesh[1..]].concat(), 8)
}

fn point_scene(records: &[Vec<u8>]) -> Arc<[u8]> {
    scene(
        &[
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"DMSH",
                &[creation(b"shape", Some(b"object")), records.concat()].concat(),
                4,
            ),
        ]
        .concat(),
    )
}

#[test]
fn cached_mesh_sparse_offsets_preserve_words_and_use_exact_f64_sum() {
    let delta = [0.25f32, -0.0, 0.5];
    for records in [
        [hint(1), tweak(b"pt[0:0]", &[delta]), cached_mesh()],
        [cached_mesh(), hint(1), tweak(b"pt[0:0]", &[delta])],
    ] {
        let bytes = point_scene(&records);
        let candidate = decode(bytes.clone()).unwrap();
        assert!(Arc::ptr_eq(
            &bytes,
            &candidate.source.as_ref().unwrap().bytes
        ));
        let mesh = candidate.shapes()[0].mesh();
        let crate::native_scene::Points::Float3(base) = mesh.points() else {
            panic!("original f32 storage required");
        };
        assert_eq!(
            base[0].map(f32::to_bits),
            [16777216.0f32, -0.0, 0.0].map(f32::to_bits)
        );
        let point = mesh.effective_point_cm(0).unwrap();
        let expected = std::array::from_fn(|i| f64::from(base[0][i]) + f64::from(delta[i]));
        assert_eq!(point.map(f64::to_bits), expected.map(f64::to_bits));
        assert_eq!(point[0], 16777216.25);
        assert_eq!(mesh.effective_point_cm(1).unwrap(), base[1].map(f64::from));
        let view = ViewScene::from_native(&candidate, 1024 * 1024).unwrap();
        assert_eq!(
            view.instances()[0].world_vertices()[0].to_array(),
            point.map(|v| v as f32)
        );
        assert_eq!(
            candidate.source.as_ref().unwrap().bytes.as_ref(),
            bytes.as_ref()
        );
    }
    let o_mesh = chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8);
    let wrong = [
        vec![hint(2), tweak(b"pt[0:0]", &[delta]), cached_mesh()],
        vec![hint(1), tweak(b"pt[0]", &[delta]), cached_mesh()],
        vec![hint(1), tweak(b"pt[1:0]", &[delta]), cached_mesh()],
        vec![
            hint(1),
            tweak(b"pt[18446744073709551616:18446744073709551616]", &[delta]),
            cached_mesh(),
        ],
        vec![hint(1), tweak(b"pt[0:1]", &[delta]), cached_mesh()],
        vec![hint(1), tweak(b"pt[0:0]", &[]), cached_mesh()],
        vec![
            hint(1),
            tweak(b"pt[0:0]", &[[f32::NAN, 0.0, 0.0]]),
            cached_mesh(),
        ],
        vec![hint(1), tweak(b"pt[3:3]", &[delta]), cached_mesh()],
        vec![
            hint(2),
            tweak(b"pt[0:0]", &[delta]),
            tweak(b"pt[0:0]", &[delta]),
            cached_mesh(),
        ],
        vec![hint(1), tweak(b"pt[0:0]", &[delta]), o_mesh],
    ];
    for records in wrong {
        assert!(decode(point_scene(&records)).is_err());
    }
}

#[test]
fn raw_attribute_must_follow_its_own_creation_record() {
    let before_attribute = scene(&group(
        b"XFRM",
        &[
            attribute(b"aDBL", b"setting"),
            creation(b"object", None),
            state(b"DBLE", b"setting"),
        ]
        .concat(),
        4,
    ));
    assert!(
        decode(before_attribute).is_err(),
        "pre-CREA ATTR must not bind to a later node"
    );
}

#[test]
fn raw_point_effects_must_follow_their_own_creation_record() {
    let before_tweak = scene(
        &[
            group(b"XFRM", &creation(b"object", None), 4),
            group(
                b"DMSH",
                &[
                    hint(1),
                    tweak(b"pt[0:0]", &[[0.25, 0.0, 0.0]]),
                    creation(b"shape", Some(b"object")),
                    cached_mesh(),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    );
    assert!(
        decode(before_tweak).is_err(),
        "pre-CREA point effects must not bind to a later mesh"
    );
}

fn connection(from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend(from);
    bytes.push(0);
    bytes.extend(to);
    bytes.push(0);
    chunk(b"CWFL", &bytes, 8)
}

fn custom_scene(fields: &[u8], connections: &[u8]) -> Arc<[u8]> {
    scene(
        &[
            group(
                b"XFRM",
                &[creation(b"object", None), fields.to_vec()].concat(),
                4,
            ),
            group(b"PCUU", &creation(b"helper", None), 4),
            group(b"CONS", &group(b"CONN", connections, 4), 4),
        ]
        .concat(),
    )
}

#[test]
fn inert_custom_definitions_bind_only_typed_owner_state_and_appearance_outputs() {
    let definitions = [
        attribute(b"aLI1", b"countSetting"),
        attribute(b"aENM", b"choiceSetting"),
        attribute(b"aDBL", b"amountSetting"),
    ]
    .concat();
    let states = [
        state(b"DBLE", b"countSetting"),
        state(b"FLGS", b"choiceSetting"),
        state(b"DBLE", b"amountSetting"),
    ]
    .concat();
    let outputs = [
        connection(b"object.choiceSetting", b"helper.i"),
        connection(b"object.amountSetting", b"helper.i"),
    ]
    .concat();
    let candidate = decode(custom_scene(
        &[definitions.clone(), states.clone()].concat(),
        &outputs,
    ))
    .unwrap();
    assert!(candidate.source.as_ref().unwrap().leaves.is_empty());
    for (from, to) in [
        (b":object.amountSetting".as_slice(), b"helper.i".as_slice()),
        (b"object.amountSetting", b":helper.i"),
        (b":object.amountSetting", b":helper.i"),
    ] {
        assert!(
            decode(custom_scene(
                &[definitions.clone(), states.clone()].concat(),
                &connection(from, to)
            ))
            .is_ok()
        );
    }
    for fields in [
        [states.clone(), definitions.clone()].concat(),
        [definitions.clone(), state(b"DBLE", b"choiceSetting")].concat(),
        [
            attribute(b"aDBL", b"translate"),
            state(b"DBLE", b"translate"),
        ]
        .concat(),
        [definitions.clone(), attribute(b"aENM", b"choiceSetting")].concat(),
        state(b"DBLE", b"missingDefinition"),
    ] {
        assert!(decode(custom_scene(&fields, &[])).is_err());
    }
    for outputs in [
        connection(b"helper.o", b"object.choiceSetting"),
        connection(b"object.amountSetting", b"helper.other"),
        connection(b"object.amountSetting", b"object.t"),
        connection(b"object.countSetting", b"helper.i"),
        connection(b":helper.o", b":object.choiceSetting"),
        connection(b":object.amountSetting", b":object.t"),
        connection(b":object.t", b"helper.i"),
        connection(b"object.t", b":helper.i"),
    ] {
        assert!(
            decode(custom_scene(
                &[definitions.clone(), states.clone()].concat(),
                &outputs
            ))
            .is_err()
        );
    }
    let unrelated = scene(
        &[
            group(
                b"XFRM",
                &[creation(b"object", None), definitions].concat(),
                4,
            ),
            group(b"XFRM", &[creation(b"other", None), states].concat(), 4),
        ]
        .concat(),
    );
    assert!(decode(unrelated).is_err());
}

#[test]
fn allocation_and_work_policies_refuse_before_expensive_finish() {
    use super::admission::{Budget, Counts};
    assert!(Budget::new(128 * 1024 * 1024).is_ok());
    assert_eq!(
        Budget::new(128 * 1024 * 1024 + 1).err().unwrap().kind(),
        ErrorKind::ResourceLimit
    );
    assert!(
        Budget::new(0)
            .unwrap()
            .preflight::<super::super::NativeFace>(usize::MAX)
            .is_err()
    );
    let mut counts = Counts {
        points: 1_562_500,
        ..Counts::default()
    };
    assert!(counts.admit_work().is_ok());
    counts.points += 1;
    assert_eq!(
        counts.admit_work().unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    assert!(
        Counts {
            leaves: usize::MAX,
            ..Counts::default()
        }
        .admit_work()
        .is_err()
    );
    let mut nodes = Vec::new();
    for index in 0..5000 {
        nodes.extend(group(
            b"XFRM",
            &creation(format!("node{index}").as_bytes(), None),
            4,
        ));
    }
    super::super::FULL_VALIDATIONS.with(|count| count.set(0));
    assert_eq!(
        decode(scene(&nodes)).unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    super::super::FULL_VALIDATIONS.with(|count| assert_eq!(count.get(), 0));
}

#[test]
fn face_headers_and_leaf_overlap_work_are_real_admission_terms() {
    use super::{
        admission::{Budget, Counts},
        binding::preflight_candidate,
    };
    let counts = Counts {
        faces: 1,
        corners: 3,
        ..Counts::default()
    };
    let cost = counts.validation_bytes().unwrap()
        + std::mem::size_of::<super::super::NativeFace>()
        + 3 * std::mem::size_of::<super::super::DirectedEdge>()
        + 4;
    assert!(
        preflight_candidate(&Budget::new(128 * 1024 * 1024 - cost).unwrap(), &counts, 0).is_ok()
    );
    assert_eq!(
        preflight_candidate(
            &Budget::new(128 * 1024 * 1024 - cost + 1).unwrap(),
            &counts,
            0
        )
        .unwrap_err()
        .kind(),
        ErrorKind::ResourceLimit
    );
    let many_sites = Counts {
        transforms: 2000,
        dag: 2000,
        sites: 2000,
        site_ancestors: 1_600_000,
        ..Counts::default()
    };
    assert_eq!(
        many_sites.admit_work().unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
    let many_leaves = Counts {
        transforms: 2000,
        dag: 2000,
        leaves: 10000,
        ..Counts::default()
    };
    assert_eq!(
        many_leaves.admit_work().unwrap_err().kind(),
        ErrorKind::ResourceLimit
    );
}

#[test]
fn chain_and_many_effective_leaves_refuse_before_finish() {
    for (count, fields) in [(5000, false), (2000, true)] {
        let mut nodes = Vec::new();
        for index in 0..count {
            let name = format!("node{index}");
            let parent = (index > 0).then(|| format!("node{}", index - 1));
            let mut children = creation(name.as_bytes(), parent.as_deref().map(str::as_bytes));
            if fields {
                for key in [b"t".as_slice(), b"r", b"s", b"rp", b"sp"] {
                    children.extend(triple(key, [1.0, 2.0, 3.0]));
                }
            }
            nodes.extend(group(b"XFRM", &children, 4));
        }
        super::super::FULL_VALIDATIONS.with(|count| count.set(0));
        assert_eq!(
            decode(scene(&nodes)).unwrap_err().kind(),
            ErrorKind::ResourceLimit
        );
        super::super::FULL_VALIDATIONS.with(|count| assert_eq!(count.get(), 0));
    }
}

#[test]
#[ignore = "requires explicit private manifest and separate SHA precheck"]
fn private_native_candidates_and_views_match_independent_counts() {
    let manifest = std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST")
        .expect("explicit private manifest required");
    let manifest = Path::new(&manifest);
    let directory = manifest.parent().unwrap().canonicalize().unwrap();
    let entries: serde_json::Value = serde_json::from_slice(&read(manifest)).unwrap();
    let display_path = std::env::var_os("AZTECCRAFT_MB_DISPLAY_RANGES")
        .expect("explicit independent display audit required");
    let wire_path = std::env::var_os("AZTECCRAFT_MB_WIRE_RANGES")
        .expect("explicit independent wire audit required");
    let display: serde_json::Value =
        serde_json::from_slice(&read(Path::new(&display_path))).unwrap();
    let wire: serde_json::Value = serde_json::from_slice(&read(Path::new(&wire_path))).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), 5);
    let nodes = [8, 13, 29, 6, 9];
    let meshes = [3, 4, 20, 1, 1];
    let points = [3900, 4084, 7529, 4875, 4996];
    let faces = [6390, 6750, 7050, 9746, 7164];
    let visible = [3, 3, 20, 1, 1];
    let mut failures = Vec::new();
    for (index, entry) in entries.as_array().unwrap().iter().enumerate() {
        let path = directory
            .join(entry["relative_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(path.starts_with(&directory));
        let bytes: Arc<[u8]> = read(&path).into();
        assert_eq!(bytes.len(), entry["bytes"].as_u64().unwrap() as usize);
        let result = decode(bytes.clone());
        if let Err(error) = &result {
            failures.push((index, *error));
            continue;
        }
        let scene = result.unwrap();
        let appearance = scene.appearance_readiness();
        assert_eq!(appearance.shapes, meshes[index]);
        if index == 0 {
            assert_eq!(
                appearance.checked_source_bindings, 3,
                "viewer source readiness: {appearance:?}"
            );
        }
        assert_eq!(scene.nodes().len(), nodes[index]);
        assert_eq!(scene.shapes().len(), meshes[index]);
        assert_eq!(
            scene
                .shapes()
                .iter()
                .map(|shape| shape.mesh().point_count())
                .sum::<usize>(),
            points[index]
        );
        assert_eq!(
            scene
                .shapes()
                .iter()
                .map(|shape| shape.mesh().faces().len())
                .sum::<usize>(),
            faces[index]
        );
        assert!(Arc::ptr_eq(&bytes, &scene.source.as_ref().unwrap().bytes));
        scene.validate().unwrap();
        independent_bindings(&scene, &bytes, index, &display, &wire);
        let view = ViewScene::from_native(&scene, 128 * 1024 * 1024).unwrap();
        assert_eq!(view.instances().len(), visible[index]);
        if index == 4 {
            let mesh = scene.shapes()[0].mesh();
            let crate::native_scene::Points::Float3(base) = mesh.points() else {
                panic!("retained base must remain f32");
            };
            for (vertex, point) in base.iter().enumerate() {
                let changed = mesh.effective_point_cm(vertex).unwrap() != point.map(f64::from);
                assert_eq!(changed, vertex < 366);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "candidate refusals by ordinal: {failures:?}"
    );
}

fn number(value: &serde_json::Value) -> usize {
    usize::try_from(value.as_u64().expect("independent audit integer")).unwrap()
}

fn independent_bindings(
    scene: &super::super::NativeScene,
    bytes: &[u8],
    index: usize,
    display: &serde_json::Value,
    wire: &serde_json::Value,
) {
    use super::super::{Points, source::TransformField};
    let audit = &display["fixtures"][index];
    let mut identities = std::collections::BTreeMap::new();
    for owner in audit["dag"].as_array().unwrap() {
        let start = number(&owner["crea"]) + 17;
        let end = bytes[start..].iter().position(|byte| *byte == 0).unwrap() + start;
        let name = std::str::from_utf8(&bytes[start..end]).unwrap();
        let id = if owner["form"] == "XFRM" {
            scene
                .nodes()
                .iter()
                .find(|node| node.name() == name)
                .unwrap()
                .transform_id()
        } else {
            scene
                .shapes()
                .iter()
                .find(|shape| shape.name() == name)
                .unwrap()
                .id()
        };
        assert!(identities.insert(number(&owner["group"]), id).is_none());
    }
    assert_eq!(scene.dag_edges().len(), identities.len());
    for owner in audit["dag"].as_array().unwrap() {
        let child = identities[&number(&owner["group"])];
        let parent = if owner["parentGroup"].is_null() {
            None
        } else {
            Some(identities[&number(&owner["parentGroup"])])
        };
        assert!(
            scene
                .dag_edges()
                .iter()
                .any(|edge| edge.child() == child && edge.parent() == parent)
        );
    }
    for unit in audit["units"].as_array().unwrap() {
        let start = number(&unit["payload"]);
        let token = &bytes[start..start + number(&unit["length"])];
        match unit["tag"].as_str().unwrap() {
            "LUNI" => {
                assert_eq!(token, b"cm");
                assert_eq!(scene.units().linear, super::super::LinearUnit::Centimeter);
            }
            "AUNI" => {
                assert_eq!(token, b"deg");
                assert_eq!(scene.units().angular, super::super::AngularUnit::Degree);
            }
            "TUNI" => assert_eq!(scene.units().time.as_bytes(), token),
            _ => panic!("unsupported audit token"),
        }
    }
    let source = scene.source.as_ref().unwrap();
    let mut expected_leaves = 0;
    for assignment in audit["fields"].as_array().unwrap() {
        if assignment["tag"] != "DBL3" {
            continue;
        }
        let field = match assignment["key"].as_str().unwrap() {
            "t" => TransformField::Translation,
            "r" => TransformField::Rotation,
            "s" => TransformField::Scale,
            "rp" => TransformField::RotatePivot,
            "sp" => TransformField::ScalePivot,
            _ => continue,
        };
        let owner = identities[&number(&assignment["group"])];
        let payload = number(&assignment["off"]) + 16;
        let numeric = payload + assignment["key"].as_str().unwrap().len() + 2;
        let (leaf_index, leaf) = source
            .leaves
            .iter()
            .enumerate()
            .find(|(_, leaf)| leaf.owner == owner && leaf.field == field)
            .unwrap();
        expected_leaves += 1;
        assert_eq!(
            leaf.payload,
            payload..payload + number(&assignment["payloadBytes"])
        );
        assert_eq!(leaf.numeric, numeric..numeric + 24);
        let node = scene
            .nodes()
            .iter()
            .find(|node| node.transform_id() == owner)
            .unwrap();
        let internal = scene.internal_transform(node.id()).unwrap();
        let actual = match field {
            TransformField::Translation => internal.translation_cm,
            TransformField::Rotation => internal.rotation_radians,
            TransformField::Scale => internal.scale,
            TransformField::RotatePivot => internal.rotate_pivot_cm,
            TransformField::ScalePivot => internal.scale_pivot_cm,
        };
        for (component, actual) in actual.iter().enumerate() {
            let start = numeric + component * 8;
            let expected = u64::from_be_bytes(bytes[start..start + 8].try_into().unwrap());
            assert!(
                actual.to_bits() == expected,
                "internal word mismatch: fixture {index}, byte {start}, component {component}"
            );
            assert_eq!(
                node.field(field)[component].origin,
                Some(super::super::source::ScalarOrigin::Binary {
                    leaf: leaf_index,
                    component
                })
            );
        }
    }
    assert_eq!(source.leaves.len(), expected_leaves);
    for mesh in wire["fixtures"][index]["mesh"].as_array().unwrap() {
        let offset = number(&mesh["chunkOffset"]);
        let owner = audit["fields"]
            .as_array()
            .unwrap()
            .iter()
            .find(|field| field["tag"] == "MESH" && number(&field["off"]) == offset)
            .unwrap();
        let shape = scene
            .shapes()
            .iter()
            .find(|shape| shape.id() == identities[&number(&owner["group"])])
            .unwrap();
        let Points::Float3(base) = shape.mesh().points() else {
            panic!("f32 base retention required");
        };
        let data = offset + 16 + number(&mesh["positionScalars"]["data"][0]);
        assert_eq!(base.len() * 3, number(&mesh["positionScalars"]["count"]));
        let tweaks = audit["fields"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|field| field["tag"] == "FLT3" && field["group"] == owner["group"])
            .map(|field| {
                let key = field["key"].as_str().unwrap();
                let (first, last) = key
                    .strip_prefix("pt[")
                    .unwrap()
                    .strip_suffix(']')
                    .unwrap()
                    .split_once(':')
                    .unwrap();
                let first: usize = first.parse().unwrap();
                let last: usize = last.parse().unwrap();
                assert_eq!((last - first + 1) * 12, number(&field["valueBytes"]));
                (first, last, number(&field["off"]) + 16 + key.len() + 2)
            })
            .collect::<Vec<_>>();
        for (vertex, point) in base.iter().enumerate() {
            let mut expected = [0.0; 3];
            for component in 0..3 {
                let start = data + vertex * 12 + component * 4;
                let word = u32::from_be_bytes(bytes[start..start + 4].try_into().unwrap());
                assert!(
                    point[component].to_bits() == word,
                    "base word mismatch: fixture {index}, vertex {vertex}, component {component}"
                );
                expected[component] = f64::from(f32::from_bits(word));
                for (first, last, values) in &tweaks {
                    if vertex < *first || vertex > *last {
                        continue;
                    }
                    let start = values + (vertex - first) * 12 + component * 4;
                    expected[component] += f64::from(f32::from_be_bytes(
                        bytes[start..start + 4].try_into().unwrap(),
                    ));
                }
            }
            assert!(
                shape
                    .mesh()
                    .effective_point_cm(vertex)
                    .unwrap()
                    .map(f64::to_bits)
                    == expected.map(f64::to_bits),
                "effective point mismatch: fixture {index}, vertex {vertex}"
            );
        }
    }
}

fn visibility(value: f64) -> Vec<u8> {
    let mut bytes = b"v\0\x24".to_vec();
    bytes.extend(value.to_be_bytes());
    chunk(b"DBLE", &bytes, 8)
}

#[test]
fn binary_transform_reports_its_recorded_local_visibility() {
    for (record, expected) in [
        (Vec::new(), Some(true)),
        (visibility(0.0), Some(false)),
        (visibility(1.0), Some(true)),
    ] {
        let bytes = scene(&group(
            b"XFRM",
            &[creation(b"object", None), record].concat(),
            4,
        ));
        let scene = decode(bytes).unwrap();
        assert_eq!(scene.nodes()[0].local_visibility(), expected);
    }
}

#[test]
fn transform_records_outside_the_modeled_channels_are_refused_at_open() {
    // The on-disk encoding of these attributes is unconfirmed by any fixture; the test only
    // pins that an undeclared record on a transform is refused, whatever its key.
    for (tag, key, kind, value) in [
        (b"DBLE", b"it".as_slice(), 0x24u8, 0.0f64),
        (b"DBLE", b"it", 0x24, 1.0),
        (b"DBLE", b"inheritsTransform", 0x24, 0.0),
        (b"DBLE", b"ro", 0x24, 2.0),
        (b"DBLE", b"shxy", 0x24, 0.5),
        (b"DBLE", b"opm", 0x24, 0.0),
    ] {
        let mut record = key.to_vec();
        record.extend([0, kind]);
        record.extend(value.to_be_bytes());
        let bytes = scene(&group(
            b"XFRM",
            &[creation(b"object", None), chunk(tag, &record, 8)].concat(),
            4,
        ));
        let error = decode(bytes).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Unsupported, "{key:?}");
    }
    for key in [b"ra".as_slice(), b"sh"] {
        let bytes = scene(&group(
            b"XFRM",
            &[creation(b"object", None), triple(key, [10.0, 0.0, 0.0])].concat(),
            4,
        ));
        assert_eq!(decode(bytes).unwrap_err().kind(), ErrorKind::Unsupported);
    }
}
