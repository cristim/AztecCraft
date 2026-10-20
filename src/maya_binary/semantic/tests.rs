mod wire_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}
use super::*;
use serde_json::Value;
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
    sync::Arc,
};
use wire_fixture::{chunk, group, mesh_payload};

fn payload<T>(
    bytes: &[u8],
    decode: impl FnOnce(&mut Cursor<'_>) -> Result<T, BinaryError>,
) -> Result<T, BinaryError> {
    let mut cursor = Cursor::new(bytes, 0..bytes.len())?;
    let result = decode(&mut cursor)?;
    cursor.finish()?;
    Ok(result)
}

fn container(children: &[u8]) -> Container {
    super::super::container::inspect(Arc::from(group(b"Maya", children, 1))).unwrap()
}

fn create(flags: u8, parent: bool) -> Vec<u8> {
    let mut bytes = vec![flags];
    bytes.extend(b"node\0");
    if parent {
        bytes.extend(b"parent\0");
    }
    bytes.extend([0x7f; 16]);
    bytes
}

fn double(triple: bool) -> Vec<u8> {
    let mut bytes = b"r\0\x20".to_vec();
    for value in [
        -std::f64::consts::FRAC_PI_2,
        -0.0,
        f64::from_bits(0x3ff0_0000_0000_0001),
    ]
    .iter()
    .take(if triple { 3 } else { 1 })
    {
        bytes.extend(value.to_bits().to_be_bytes());
    }
    bytes
}

#[test]
fn create_layout_reserves_identity_and_preserves_every_admitted_flag() {
    for flags in [0x04, 0x05, 0x0c, 0x80, 0x81, 0x8c, 0x8d] {
        let has_parent = flags < 0x80;
        let bytes = create(flags, has_parent);
        let record = payload(&bytes, records::decode_create).unwrap();
        assert_eq!(record.flags, flags);
        assert_eq!(&bytes[record.name], b"node");
        assert_eq!(record.parent.is_some(), has_parent);
        assert_eq!(&bytes[record.identity], &[0x7f; 16]);
    }
    let mut missing_nul = create(0x80, false);
    missing_nul[5] = b'x';
    missing_nul[6] = 0;
    assert_eq!(
        payload(&missing_nul, records::decode_create)
            .unwrap_err()
            .kind,
        ErrorKind::Truncated
    );
    assert_eq!(
        payload(&create(0, false), records::decode_create)
            .unwrap_err()
            .kind,
        ErrorKind::Unsupported
    );
}

#[test]
fn doubles_preserve_radian_signed_zero_and_full_precision_words() {
    let bytes = double(true);
    let record = payload(&bytes, |cursor| records::decode_double(cursor, true)).unwrap();
    assert_eq!(&bytes[record.key], b"r");
    assert_eq!(record.kind, 0x20);
    let words: Vec<_> = bytes[record.numeric]
        .as_chunks::<8>()
        .0
        .iter()
        .map(|word| u64::from_be_bytes(*word))
        .collect();
    assert_eq!(
        words,
        [
            (-std::f64::consts::FRAC_PI_2).to_bits(),
            (-0.0f64).to_bits(),
            0x3ff0_0000_0000_0001
        ]
    );
    for kind in [0x20, 0x24, 0x30] {
        let mut bytes = double(false);
        bytes[2] = kind;
        assert_eq!(
            payload(&bytes, |cursor| records::decode_double(cursor, false))
                .unwrap()
                .kind,
            kind
        );
        bytes.push(0);
        assert_eq!(
            payload(&bytes, |cursor| records::decode_double(cursor, false))
                .unwrap_err()
                .kind,
            ErrorKind::Layout
        );
    }
    assert!(
        payload(&bytes[..bytes.len() - 1], |cursor| records::decode_double(
            cursor, true
        ))
        .is_err()
    );
}

#[test]
fn connections_and_relations_require_exact_mode_count_and_terminators() {
    for mode in [0, 1] {
        let mut bytes = vec![mode];
        bytes.extend(b"source.out\0target.in\0");
        let record = payload(&bytes, records::decode_connection).unwrap();
        assert_eq!(record.mode, mode);
        assert_eq!(&bytes[record.source], b"source.out");
        assert_eq!(&bytes[record.destination], b"target.in");
        bytes.push(0);
        assert!(payload(&bytes, records::decode_connection).is_err());
    }
    let mut relation = b"link\0head\0".to_vec();
    relation.extend(2u32.to_be_bytes());
    relation.extend(b"first\0second\0");
    let record = payload(&relation, records::decode_relation).unwrap();
    assert_eq!(&relation[record.kind], b"link");
    assert_eq!(&relation[record.head], b"head");
    assert_eq!(&relation[record.tails[0].clone()], b"first");
    assert_eq!(&relation[record.tails[1].clone()], b"second");
    relation[record.count_word].copy_from_slice(&3u32.to_be_bytes());
    assert_eq!(
        payload(&relation, records::decode_relation)
            .unwrap_err()
            .kind,
        ErrorKind::Unsupported
    );
    assert!(payload(&[2, b'a', 0, b'b', 0], records::decode_connection).is_err());
}

#[test]
fn selected_default_scope_never_inherits_the_preceding_created_node() {
    let mut children = group(b"XFRM", &chunk(b"CREA", &create(0x80, false), 8), 4);
    let mut selected = chunk(b"SLCT", b":default", 8);
    selected.extend(chunk(b"DBL3", &double(true), 8));
    children.extend(group(b"SLCT", &selected, 4));
    let container = container(&children);
    let facts = decode_records(&container).unwrap();
    assert!(std::ptr::eq(facts.container, &container));
    assert_eq!(facts.records[2].owner, Some(Owner::Selected(1)));
    assert_ne!(facts.records[2].owner, Some(Owner::Created(0)));
    let Data::Selection(target) = &facts.records[1].data else {
        panic!("expected selection")
    };
    assert_eq!(&container.source()[target.clone()], b":default");
    let mut late = chunk(b"DBL3", &double(true), 8);
    late.extend(chunk(b"SLCT", b":default", 8));
    assert_eq!(
        decode_records(&self::container(&group(b"SLCT", &late, 4)))
            .unwrap_err()
            .kind,
        ErrorKind::Owner
    );
    let mut late = chunk(b"OPAQ", &[1, 2], 8);
    late.extend(chunk(b"SLCT", b":default", 8));
    late.extend(chunk(b"DBL3", &double(true), 8));
    assert_eq!(
        decode_records(&self::container(&group(b"SLCT", &late, 4)))
            .unwrap_err()
            .kind,
        ErrorKind::Owner
    );
    selected.extend(chunk(b"SLCT", b":other", 8));
    assert_eq!(
        decode_records(&self::container(&group(b"SLCT", &selected, 4)))
            .unwrap_err()
            .kind,
        ErrorKind::Owner
    );
    assert!(decode_records(&self::container(&group(b"SLCT", &[], 4))).is_err());
    let mut duplicate = chunk(b"CREA", &create(0x80, false), 8);
    duplicate.extend(chunk(b"CREA", &create(0x80, false), 8));
    assert!(decode_records(&self::container(&group(b"XFRM", &duplicate, 4))).is_err());
}

#[test]
fn connection_records_require_one_list_and_exact_group_ancestry() {
    let connection = chunk(b"CWFL", b"\0source.out\0target.in\0", 8);
    let mut relation = b"link\0head\0".to_vec();
    relation.extend(2u32.to_be_bytes());
    relation.extend(b"first\0second\0");
    let relation = chunk(b"RELA", &relation, 8);
    let mut leaves = connection.clone();
    leaves.extend(&relation);
    let list = group(b"CONN", &leaves, 4);
    let valid = container(&group(b"CONS", &list, 4));
    assert_eq!(decode_records(&valid).unwrap().records.len(), 2);
    for leaf in [&connection, &relation] {
        for wrong in [group(b"XFRM", leaf, 4), group(b"CONN", leaf, 4)] {
            assert_eq!(
                decode_records(&container(&wrong)).unwrap_err().kind,
                ErrorKind::Owner
            );
        }
        let wrong_parent = group(b"CONS", &group(b"XFRM", leaf, 4), 4);
        assert_eq!(
            decode_records(&container(&wrong_parent)).unwrap_err().kind,
            ErrorKind::Owner
        );
    }
    let mut duplicate = group(b"CONS", &list, 4);
    duplicate.extend(group(b"CONS", &[], 4));
    assert_eq!(
        decode_records(&container(&duplicate)).unwrap_err().kind,
        ErrorKind::Owner
    );
    let mut duplicate = list;
    duplicate.extend(group(b"CONN", &[], 4));
    assert_eq!(
        decode_records(&container(&group(b"CONS", &duplicate, 4)))
            .unwrap_err()
            .kind,
        ErrorKind::Owner
    );
}

#[test]
fn mesh_wire_flags_counts_normals_uvs_and_trailer_are_preserved() {
    for (flag, reverse, terminal) in [
        (0, false, false),
        (0x80, true, false),
        (0x60, false, true),
        (0xe0, true, true),
    ] {
        assert_eq!(
            mesh::corner((flag << 24) | 7, 0).unwrap(),
            (7, reverse, terminal)
        );
    }
    let bytes = mesh_payload(&[0, 1, 0x6000_0002]);
    let record = payload(&bytes, mesh::decode_mesh).unwrap();
    assert_eq!(&bytes[record.key.clone()], b"o");
    assert_eq!(bytes[record.kind_offset], 0x20);
    assert_eq!(record.positions.count, 9);
    assert_eq!(record.endpoints.values(&bytes).next(), Some(0x8000_0000));
    assert_eq!(record.corners.values(&bytes).last(), Some(0x6000_0002));
    assert_eq!(
        record.normals.values(&bytes).next(),
        Some(1e20f32.to_bits())
    );
    assert_eq!(record.uv_header.start, record.normals.range.end);
    assert_eq!(record.uv_header.len(), 12);
    assert_eq!(&bytes[record.uv_name.clone()], b"uv");
    assert_eq!(record.uv_points.count, 6);
    assert_eq!(record.uv_ids.values(&bytes).collect::<Vec<_>>(), [0, 1, 2]);
    assert_eq!(&bytes[record.trailer.clone()], &[0; 20]);
    for end in 0..bytes.len() {
        assert!(payload(&bytes[..end], mesh::decode_mesh).is_err());
    }
    let mut bad = bytes.clone();
    bad[3..7].copy_from_slice(&8u32.to_be_bytes());
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Layout
    );
    let mut bad = bytes.clone();
    bad[record.corners.range.start..record.corners.range.start + 4]
        .copy_from_slice(&0x4000_0000u32.to_be_bytes());
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let mut bad = bytes.clone();
    bad[record.corners.range.start..record.corners.range.start + 4]
        .copy_from_slice(&3u32.to_be_bytes());
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Reference
    );
    let mut bad = bytes.clone();
    bad[record.uv_ids.range.start..record.uv_ids.range.start + 4]
        .copy_from_slice(&3u32.to_be_bytes());
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Reference
    );
    let mut bad = bytes.clone();
    *bad.last_mut().unwrap() = 1;
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Unsupported
    );
    let mut bad = bytes;
    bad.extend([0; 12]);
    assert_eq!(
        payload(&bad, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Layout
    );
}

#[test]
fn an_empty_uv_set_is_admitted_only_when_both_points_and_ids_are_empty() {
    let corners = [0, 1, 0x6000_0002];
    let bytes = wire_fixture::mesh_payload_with_uv(&corners, &[], &[]);
    let record = payload(&bytes, mesh::decode_mesh).unwrap();
    assert_eq!((record.uv_points.count, record.uv_ids.count), (0, 0));
    assert_eq!(record.corners.count, 3);
    assert_eq!(&bytes[record.uv_name.clone()], b"uv");
    let points_only = wire_fixture::mesh_payload_with_uv(&corners, &[0, 0, 0, 0, 0, 0], &[]);
    assert_eq!(
        payload(&points_only, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Layout
    );
    let ids_only = wire_fixture::mesh_payload_with_uv(&corners, &[], &[0, 1, 2]);
    assert_eq!(
        payload(&ids_only, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Reference
    );
    let wrong_count = wire_fixture::mesh_payload_with_uv(&corners, &[0, 0], &[0]);
    assert_eq!(
        payload(&wrong_count, mesh::decode_mesh).unwrap_err().kind,
        ErrorKind::Layout
    );
}

#[test]
fn metadata_reservations_are_bounded_and_debug_never_expands_source() {
    let count = MAX_FACT_METADATA_BYTES / size_of::<Record>();
    assert!(metadata_bytes(count, 0).unwrap() <= MAX_FACT_METADATA_BYTES);
    assert_eq!(
        metadata_bytes(count + 1, 0).unwrap_err().kind,
        ErrorKind::Resource
    );
    assert_eq!(
        metadata_bytes(usize::MAX, 1).unwrap_err().kind,
        ErrorKind::Resource
    );
    assert_eq!(
        metadata_bytes(1, usize::MAX).unwrap_err().kind,
        ErrorKind::Resource
    );
    let source = super::super::container::inspect(Arc::from(wire_fixture::specimen())).unwrap();
    let facts = decode_records(&source).unwrap();
    assert!(!format!("{facts:?}").contains(&format!("{:?}", source.source())));
    assert!(
        metadata_bytes(facts.records.capacity(), facts.owners.capacity()).unwrap()
            <= MAX_FACT_METADATA_BYTES
    );
}

fn bounded_read(path: &Path) -> Vec<u8> {
    let mut bytes = Vec::new();
    File::open(path)
        .expect("private input must exist")
        .take(super::super::container::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .expect("private input must be readable");
    assert!(bytes.len() <= super::super::container::MAX_BYTES);
    bytes
}

fn number(value: &Value) -> usize {
    usize::try_from(value.as_u64().expect("expected audit integer")).unwrap()
}
fn span(value: &Value, base: usize) -> Span {
    base + number(&value[0])..base + number(&value[1])
}

fn audit_words(actual: &Words, expected: &Value, base: usize, count_name: &str, data_name: &str) {
    assert_eq!(actual.count, number(&expected[count_name]));
    assert_eq!(actual.range, span(&expected[data_name], base));
}

fn independent_mesh_trace(mesh: &mesh::Mesh, source: &[u8]) -> (usize, usize, usize) {
    for word in mesh.positions.values(source) {
        assert!(f32::from_bits(word).is_finite());
    }
    let mut start = None;
    let mut last = 0;
    let mut corners = 0;
    let mut faces = 0;
    for word in mesh.corners.values(source) {
        let edge = (word & 0x00ff_ffff) as usize;
        let data = mesh.endpoints.range.start + edge * 8;
        let a =
            (u32::from_be_bytes(source[data..data + 4].try_into().unwrap()) & 0x7fff_ffff) as usize;
        let b = u32::from_be_bytes(source[data + 4..data + 8].try_into().unwrap()) as usize;
        assert!(a < mesh.positions.count / 3 && b < mesh.positions.count / 3);
        let (from, to) = if word & 0x8000_0000 != 0 {
            (b, a)
        } else {
            (a, b)
        };
        if start.is_none() {
            start = Some(from);
        } else {
            assert_eq!(from, last);
        }
        last = to;
        corners += 1;
        if word & 0x6000_0000 == 0x6000_0000 {
            assert_eq!(Some(to), start);
            assert_eq!(corners, 3);
            faces += 1;
            corners = 0;
            start = None;
        }
    }
    assert!(start.is_none());
    (mesh.positions.count / 3, mesh.endpoints.count / 2, faces)
}

#[test]
#[ignore = "requires explicit private fixture/audit manifests and separate SHA precheck"]
fn private_wire_fixture_spans_and_mesh_trace_match_audit() {
    let input = std::env::var_os("AZTECCRAFT_MB_FIXTURE_MANIFEST")
        .expect("explicit fixture manifest required");
    let wire =
        std::env::var_os("AZTECCRAFT_MB_WIRE_RANGES").expect("explicit wire manifest required");
    let input = Path::new(&input);
    let manifest: Value = serde_json::from_slice(&bounded_read(input)).unwrap();
    let audit: Value = serde_json::from_slice(&bounded_read(Path::new(&wire))).unwrap();
    assert_eq!(manifest.as_array().unwrap().len(), 5);
    assert_eq!(audit["fixtures"].as_array().unwrap().len(), 5);
    let expected_meshes = [3, 4, 20, 1, 1];
    let expected_vertices = [3900, 4084, 7529, 4875, 4996];
    let expected_faces = [6390, 6750, 7050, 9746, 7164];
    for (ordinal, fixture) in manifest.as_array().unwrap().iter().enumerate() {
        let directory = input.parent().unwrap().canonicalize().unwrap();
        let relative = Path::new(fixture["relative_path"].as_str().unwrap());
        assert!(
            relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        );
        let candidate = directory.join(relative).canonicalize().unwrap();
        assert!(candidate.starts_with(&directory));
        let bytes = bounded_read(&candidate);
        assert_eq!(bytes.len(), number(&fixture["bytes"]));
        let source = super::super::container::inspect(Arc::from(bytes)).unwrap();
        let facts = decode_records(&source).unwrap();
        assert!(std::ptr::eq(facts.container, &source));
        let expected = &audit["fixtures"][ordinal];
        let mut counts = [0; 6];
        let mut vertex_total = 0;
        let mut face_total = 0;
        for record in &facts.records {
            let chunk = source.chunk(record.chunk).unwrap();
            assert_eq!(chunk.parent(), Some(record.group));
            let offset = chunk.header_range().start;
            let base = chunk.payload_range().start;
            match &record.data {
                Data::Create(value) => {
                    let row = expected["crea"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row[0]) == offset)
                        .unwrap();
                    assert_eq!(value.flags as usize, number(&row[2]));
                    assert_eq!(value.name.end - base, number(&row[3]));
                    assert_eq!(
                        value.parent.as_ref().map(|range| range.end - base),
                        row[4].as_u64().map(|value| value as usize)
                    );
                    assert_eq!(
                        value.identity,
                        base + number(&row[5])..chunk.payload_range().end
                    );
                    counts[0] += 1;
                }
                Data::Double(value) => {
                    let row = expected["double"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row[0]) == offset)
                        .unwrap();
                    assert_eq!(value.key.end - base, number(&row[3]));
                    assert_eq!(value.kind as usize, number(&row[4]));
                    assert_eq!(
                        value.numeric,
                        base + number(&row[5])..chunk.payload_range().end
                    );
                    let group = source.chunk(record.group).unwrap();
                    if group.form() == Some(*b"SLCT") {
                        assert!(matches!(record.owner, Some(Owner::Selected(_))));
                    } else {
                        assert!(matches!(record.owner, Some(Owner::Created(_))));
                    }
                    counts[1] += 1;
                }
                Data::Connection(value) => {
                    let row = expected["cwfl"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row[0]) == offset)
                        .unwrap();
                    assert_eq!(value.mode as usize, number(&row[2]));
                    assert_eq!(value.source.end - base, number(&row[3]));
                    assert_eq!(value.destination.end - base, number(&row[4]));
                    counts[2] += 1;
                }
                Data::Relation { value } => {
                    let row = expected["rela"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row[0]) == offset)
                        .unwrap();
                    assert_eq!(value.kind.end - base, number(&row[3]));
                    assert_eq!(value.head.end - base, number(&row[4]));
                    assert_eq!(value.count_word.start - base, number(&row[5]));
                    assert_eq!(value.tails[0].end - base, number(&row[7]));
                    assert_eq!(value.tails[1].end - base, number(&row[8]));
                    counts[3] += 1;
                }
                Data::Selection(value) => {
                    let row = expected["slct"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row[0]) == offset)
                        .unwrap();
                    assert_eq!(value.len(), number(&row[1]));
                    counts[4] += 1;
                }
                Data::Mesh(value) => {
                    let row = expected["mesh"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|row| number(&row["chunkOffset"]) == offset)
                        .unwrap();
                    assert_eq!(value.key, span(&row["key"], base));
                    assert_eq!(value.kind_offset - base, number(&row["kindOffset"]));
                    audit_words(
                        &value.positions,
                        &row["positionScalars"],
                        base,
                        "count",
                        "data",
                    );
                    audit_words(&value.endpoints, &row["edgeWords"], base, "count", "data");
                    audit_words(&value.corners, &row["faceWords"], base, "count", "data");
                    audit_words(&value.normals, &row["normalScalars"], base, "count", "data");
                    assert_eq!(value.uv_header.start - base, number(&row["uvHeaderOffset"]));
                    let set = &row["sets"][0];
                    assert_eq!(value.uv_name, span(&set["nameRange"], base));
                    audit_words(&value.uv_points, set, base, "pointScalars", "points");
                    audit_words(&value.uv_ids, set, base, "assignments", "ids");
                    assert_eq!(value.trailer, span(&row["trailer"], base));
                    let (vertices, _, faces) = independent_mesh_trace(value, source.source());
                    vertex_total += vertices;
                    face_total += faces;
                    counts[5] += 1;
                }
            }
        }
        for (count, name) in counts
            .iter()
            .zip(["crea", "double", "cwfl", "rela", "slct", "mesh"])
        {
            assert_eq!(*count, expected[name].as_array().unwrap().len());
        }
        assert_eq!(counts[5], expected_meshes[ordinal]);
        assert_eq!(vertex_total, expected_vertices[ordinal]);
        assert_eq!(face_total, expected_faces[ordinal]);
    }
}
