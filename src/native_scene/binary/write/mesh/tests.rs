use super::*;
use crate::native_scene::{
    DirectedEdge, NativeEdge, NativeFace, NativeScene,
    binary::write::{
        chunk::group as framed,
        fixtures::{MAYA_2025, groups, leaves, walk},
        profile::ROOT_AUX,
    },
};
use crate::scene::Primitive;

const UUID: Uuid = Uuid::from_bytes([0x5a; 16]);

/// A mesh whose faces share an edge when two loops walk the same vertex pair.
fn mesh_of(points: &[[f32; 3]], loops: &[&[usize]]) -> NativeMesh {
    let mut edges: Vec<NativeEdge> = Vec::new();
    let mut faces = Vec::new();
    for vertices in loops {
        let mut outer = Vec::new();
        for (i, &a) in vertices.iter().enumerate() {
            let b = vertices[(i + 1) % vertices.len()];
            let found = edges.iter().position(|e| {
                let [x, y] = e.endpoints();
                (x, y) == (a, b) || (x, y) == (b, a)
            });
            let edge = found.unwrap_or_else(|| {
                edges.push(NativeEdge {
                    endpoints: [a, b],
                    smooth: None,
                });
                edges.len() - 1
            });
            outer.push(DirectedEdge {
                edge,
                reversed: edges[edge].endpoints() != [a, b],
            });
        }
        faces.push(NativeFace {
            outer,
            holes: Vec::new(),
        });
    }
    NativeMesh::from_binary_parts(points.to_vec(), edges, faces, Vec::new()).unwrap()
}

const CUBE_POINTS: [[f32; 3]; 8] = [
    [0.0, 0.0, 0.0],
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 0.0, 1.0],
    [1.0, 0.0, 1.0],
    [1.0, 1.0, 1.0],
    [0.0, 1.0, 1.0],
];
const CUBE_FACES: [&[usize]; 6] = [
    &[0, 3, 2, 1],
    &[4, 5, 6, 7],
    &[0, 1, 5, 4],
    &[1, 2, 6, 5],
    &[2, 3, 7, 6],
    &[3, 0, 4, 7],
];

/// The payload read back by an independent parser that shares nothing with the writer.
struct Parsed {
    key: Vec<u8>,
    kind: u8,
    points: Vec<u32>,
    endpoints: Vec<u32>,
    corners: Vec<u32>,
    normals: Vec<u32>,
    uv_header: [u32; 3],
    uv_name: Vec<u8>,
    uv_points: Vec<u32>,
    uv_ids: Vec<u32>,
    trailer: Vec<u8>,
}

fn parse(payload: &[u8]) -> Parsed {
    let nul = |at: usize| at + payload[at..].iter().position(|b| *b == 0).unwrap();
    let key_end = nul(0);
    let mut at = key_end + 1;
    let kind = payload[at];
    at += 1;
    let word = |at: &mut usize| {
        let value = u32::from_be_bytes(payload[*at..*at + 4].try_into().unwrap());
        *at += 4;
        value
    };
    let array = |at: &mut usize| {
        let count = word(at) as usize;
        (0..count).map(|_| word(at)).collect::<Vec<_>>()
    };
    let points = array(&mut at);
    let endpoints = array(&mut at);
    let corners = array(&mut at);
    let normals = array(&mut at);
    let uv_header = [word(&mut at), word(&mut at), word(&mut at)];
    let name_end = nul(at);
    let uv_name = payload[at..name_end].to_vec();
    at = name_end + 1;
    let uv_points = array(&mut at);
    let uv_ids = array(&mut at);
    Parsed {
        key: payload[..key_end].to_vec(),
        kind,
        points,
        endpoints,
        corners,
        normals,
        uv_header,
        uv_name,
        uv_points,
        uv_ids,
        trailer: payload[at..].to_vec(),
    }
}

fn flags(mesh: &NativeMesh) -> Vec<bool> {
    parse(&mesh_payload(mesh).unwrap())
        .endpoints
        .chunks(2)
        .map(|pair| pair[0] & 0x8000_0000 != 0)
        .collect()
}

fn cube() -> NativeMesh {
    mesh_of(&CUBE_POINTS, &CUBE_FACES)
}

#[test]
fn the_payload_is_key_kind_then_counted_arrays_and_the_empty_map1_set() {
    let mesh = cube();
    let parsed = parse(&mesh_payload(&mesh).unwrap());
    assert_eq!(
        (parsed.key.as_slice(), parsed.kind),
        (b"o".as_slice(), 0x20)
    );
    let expected: Vec<u32> = CUBE_POINTS.iter().flatten().map(|v| v.to_bits()).collect();
    assert_eq!(parsed.points, expected);
    assert_eq!(parsed.endpoints.len(), 24);
    for (pair, edge) in parsed.endpoints.chunks(2).zip(mesh.edges()) {
        assert_eq!(
            [pair[0] & 0x7fff_ffff, pair[1]].map(|v| v as usize),
            edge.endpoints()
        );
    }
    assert_eq!(parsed.corners.len(), 24);
    assert!(parsed.normals.is_empty());
    assert_eq!(parsed.uv_header, [0, 1, 0]);
    assert_eq!(parsed.uv_name, b"map1");
    assert!(parsed.uv_points.is_empty() && parsed.uv_ids.is_empty());
    assert_eq!(parsed.trailer, [0; 20]);
}

#[test]
fn corner_words_carry_the_edge_the_reverse_bit_and_one_terminal_per_face() {
    let mesh = cube();
    let corners = parse(&mesh_payload(&mesh).unwrap()).corners;
    let mut at = 0;
    for face in mesh.faces() {
        for (index, directed) in face.outer().iter().enumerate() {
            let word = corners[at];
            at += 1;
            assert_eq!((word & 0xff_ffff) as usize, directed.edge);
            assert_eq!(word & 0x8000_0000 != 0, directed.reversed);
            let terminal = word & 0x6000_0000 == 0x6000_0000;
            assert_eq!(terminal, index + 1 == face.outer().len());
        }
    }
    assert_eq!(at, corners.len());
    assert!(corners.iter().any(|w| w & 0x8000_0000 != 0));
}

#[test]
fn bit31_is_set_on_boundary_edges_and_clear_on_interior_edges() {
    // PROVISIONAL rule from the private corpus (99.6% of boundary edges set).
    let scene_mesh = |primitive| {
        let mut scene = NativeScene::default();
        scene.create(primitive, "m").unwrap();
        scene.shapes()[0].mesh().clone()
    };
    let plane = flags(&scene_mesh(Primitive::Plane));
    assert_eq!(plane.len(), 4);
    assert!(plane.iter().all(|set| *set));
    assert!(flags(&scene_mesh(Primitive::Cube)).iter().all(|set| !set));
    assert!(flags(&cube()).iter().all(|set| !set));
}

#[test]
fn an_open_mesh_marks_exactly_the_edges_of_its_hole_and_non_manifold_edges() {
    let open = mesh_of(
        &CUBE_POINTS,
        &[
            CUBE_FACES[0],
            CUBE_FACES[2],
            CUBE_FACES[3],
            CUBE_FACES[4],
            CUBE_FACES[5],
        ],
    );
    let set: Vec<[usize; 2]> = open
        .edges()
        .iter()
        .zip(flags(&open))
        .filter(|(_, set)| *set)
        .map(|(edge, _)| edge.endpoints())
        .collect();
    let mut hole = [[4, 5], [5, 6], [6, 7], [7, 4]].map(|[a, b]: [usize; 2]| [a.min(b), a.max(b)]);
    let mut got: Vec<_> = set.iter().map(|[a, b]| [*a.min(b), *a.max(b)]).collect();
    hole.sort();
    got.sort();
    assert_eq!(got, hole);
    // Three triangles on one edge: that edge has three uses, and every other edge one.
    let fan = mesh_of(
        &[
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, -1.0, 0.0],
        ],
        &[&[0, 1, 2], &[1, 0, 3], &[0, 1, 4]],
    );
    assert!(flags(&fan).iter().all(|set| *set));
}

#[test]
fn the_dmsh_group_is_crea_then_mesh_with_the_shape_name_parent_and_uuid() {
    let bytes = framed(
        *b"Maya",
        ROOT_AUX,
        &[dmsh("cubeShape", "cube", UUID, &cube()).unwrap()],
        Align::One,
    )
    .unwrap();
    let entries = walk(&bytes);
    let kids: Vec<_> = entries
        .iter()
        .filter(|e| e.parent_form == Some(*b"DMSH"))
        .collect();
    assert_eq!(
        kids.iter().map(|e| e.tag).collect::<Vec<_>>(),
        [*b"CREA", *b"MESH"]
    );
    assert_eq!((kids[0].aux, kids[1].aux), (SHAPE_CREA_AUX, MESH_AUX));
    assert_eq!(groups(&entries, b"DMSH")[0].aux, DMSH_GROUP_AUX);
    let mut expected = vec![0x04];
    expected.extend_from_slice(b"cubeShape\0cube\0");
    expected.extend_from_slice(&UUID.to_bytes_le());
    assert_eq!(kids[0].bytes(&bytes), expected);
}

#[test]
fn the_aux_words_and_flag_are_what_the_fixture_stores() {
    let entries = walk(MAYA_2025);
    let mesh = leaves(&entries, b"MESH", b"DMSH");
    assert_eq!(mesh.len(), 1);
    assert_eq!((mesh[0].start, mesh[0].aux), (0x11324, MESH_AUX));
    let dmsh = groups(&entries, b"DMSH");
    assert_eq!(dmsh.len(), 2);
    // The plain shape (no MESH) is the pair a fresh plain shape takes.
    assert_eq!((dmsh[0].start, dmsh[0].aux), (0xe5c, DMSH_GROUP_AUX));
    let crea = leaves(&entries, b"CREA", b"DMSH");
    assert_eq!((crea[0].start, crea[0].aux), (0xe70, SHAPE_CREA_AUX));
    // The skinned intermediate shape that holds the MESH differs, so it is not used.
    assert_eq!((dmsh[1].start, dmsh[1].aux), (0xfd8, 0x4d02_0000));
    assert_eq!((crea[1].start, crea[1].aux), (0xfec, 0x4702_0000));
    assert!(
        crea.iter()
            .all(|c| c.bytes(MAYA_2025)[0] == SHAPE_CREA_FLAGS)
    );
    let io = leaves(&entries, b"DBLE", b"DMSH")
        .into_iter()
        .filter(|c| c.start > dmsh[1].start && c.bytes(MAYA_2025).starts_with(b"io\0"))
        .count();
    assert_eq!(io, 1, "the fixture's MESH shape is an intermediate object");
}

#[test]
fn the_fixture_mesh_has_the_uv_header_set_name_and_trailer_the_writer_emits() {
    let entries = walk(MAYA_2025);
    let payload = leaves(&entries, b"MESH", b"DMSH")[0]
        .bytes(MAYA_2025)
        .to_vec();
    let fixture = parse(&payload);
    assert_eq!(
        (fixture.key.as_slice(), fixture.kind),
        (b"ci".as_slice(), 0x20)
    );
    assert_ne!(
        fixture.key, MESH_KEY,
        "the fixture key belongs to an intermediate shape"
    );
    assert_eq!(fixture.uv_header, UV_HEADER);
    assert_eq!(fixture.uv_name, UV_SET);
    assert_eq!(fixture.trailer, [0; TRAILER_BYTES]);
}
