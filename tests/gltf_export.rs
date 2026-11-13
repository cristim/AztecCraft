//! glTF export oracle: expected values are computed by hand from the Khronos glTF 2.0 spec
//! (accessor min/max, byte lengths, index ranges, CCW winding), and the container and base64 are
//! decoded here by code independent of the exporter.

// The f32 literals below are hand-computed decimal values; the compiler rounds each to the f32
// the exporter must also produce.
#![allow(clippy::excessive_precision)]

use azteccraft::{
    gltf::{GltfError, GltfSummary, export_native_new},
    native_scene::{Channel, MaterialChoice, NativeScene, UvProjectAxis},
    scene::Primitive,
    view_scene::{ViewInstance, ViewScene},
};
use serde_json::Value;
use std::fs;

const BUDGET: usize = 64 * 1024 * 1024;

fn scene_with(primitives: &[(Primitive, &str)]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (primitive, name) in primitives {
        scene.create(*primitive, name).unwrap();
    }
    scene
}

fn export(scene: &NativeScene, file: &str) -> (Result<GltfSummary, GltfError>, std::path::PathBuf) {
    let view = ViewScene::from_native(scene, BUDGET).unwrap();
    let instances: Vec<&ViewInstance> = view.instances().iter().collect();
    let directory = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let path = directory.path().join(file);
    (export_native_new(&path, scene, &instances, &|| true), path)
}

fn decode_base64(text: &str) -> Vec<u8> {
    const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let sextets: Vec<u32> = text
        .chars()
        .map(|c| ALPHABET.find(c).expect("base64 alphabet only, no padding") as u32)
        .collect();
    assert_eq!(sextets.len() % 4, 0);
    sextets
        .chunks(4)
        .flat_map(|q| {
            let word = q[0] << 18 | q[1] << 12 | q[2] << 6 | q[3];
            [(word >> 16) as u8, (word >> 8) as u8, word as u8]
        })
        .collect()
}

struct Loaded {
    json: Value,
    buffer: Vec<u8>,
}

fn load_gltf(path: &std::path::Path) -> Loaded {
    let json: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let uri = json["buffers"][0]["uri"].as_str().unwrap();
    let data = uri
        .strip_prefix("data:application/octet-stream;base64,")
        .unwrap();
    Loaded {
        buffer: decode_base64(data),
        json,
    }
}

fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Positions and triangles of mesh `mesh`, read through the accessors as the spec describes.
fn mesh_data(loaded: &Loaded, mesh: usize) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let primitive = &loaded.json["meshes"][mesh]["primitives"][0];
    let section = |accessor: &Value| {
        let accessor_index = accessor.as_u64().unwrap() as usize;
        let accessor = &loaded.json["accessors"][accessor_index];
        let view = &loaded.json["bufferViews"][accessor["bufferView"].as_u64().unwrap() as usize];
        (
            accessor["count"].as_u64().unwrap() as usize,
            view["byteOffset"].as_u64().unwrap() as usize,
        )
    };
    let (count, offset) = section(&primitive["attributes"]["POSITION"]);
    let positions = (0..count)
        .map(|i| std::array::from_fn(|axis| f32_at(&loaded.buffer, offset + i * 12 + axis * 4)))
        .collect();
    let (count, offset) = section(&primitive["indices"]);
    let triangles = (0..count / 3)
        .map(|t| std::array::from_fn(|k| u32_at(&loaded.buffer, offset + (t * 3 + k) * 4)))
        .collect();
    (positions, triangles)
}

fn as_f32(value: &Value) -> f32 {
    value.as_f64().unwrap() as f32
}

fn assert_outward(positions: &[[f32; 3]], triangles: &[[u32; 3]]) {
    let centre = positions.iter().fold([0.0f32; 3], |sum, p| {
        std::array::from_fn(|a| sum[a] + p[a] / positions.len() as f32)
    });
    for triangle in triangles {
        let [a, b, c] = triangle.map(|i| positions[i as usize]);
        let (u, v): ([f32; 3], [f32; 3]) = (
            std::array::from_fn(|k| b[k] - a[k]),
            std::array::from_fn(|k| c[k] - a[k]),
        );
        let normal = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let outward: f32 = (0..3).map(|k| normal[k] * (a[k] - centre[k])).sum();
        assert!(outward > 0.0, "triangle {triangle:?} faces inward");
    }
}

#[test]
fn unit_cube_gltf_matches_hand_computed_accessors() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let (summary, path) = export(&scene, "cube.gltf");
    assert_eq!(
        summary.unwrap(),
        GltfSummary {
            objects: 1,
            vertices: 8,
            triangles: 12
        }
    );
    let loaded = load_gltf(&path);
    let json = &loaded.json;
    assert_eq!(json["asset"]["version"], "2.0");
    // 8 vertices * 12 bytes + 12 triangles * 3 indices * 4 bytes.
    assert_eq!(json["buffers"][0]["byteLength"], 96 + 144);
    assert_eq!(loaded.buffer.len(), 240);
    assert_eq!(json["bufferViews"][0]["byteLength"], 96);
    assert_eq!(json["bufferViews"][0]["target"], 34962);
    assert_eq!(json["bufferViews"][1]["byteOffset"], 96);
    assert_eq!(json["bufferViews"][1]["byteLength"], 144);
    assert_eq!(json["bufferViews"][1]["target"], 34963);
    let position = &json["accessors"][0];
    assert_eq!(
        (position["componentType"].clone(), position["count"].clone()),
        (5126.into(), 8.into())
    );
    // The 1 cm cube spans +-0.5 cm = +-0.005 m.
    for axis in 0..3 {
        assert_eq!(as_f32(&position["min"][axis]), -0.005f32);
        assert_eq!(as_f32(&position["max"][axis]), 0.005f32);
    }
    let indices = &json["accessors"][1];
    assert_eq!(
        (indices["componentType"].clone(), indices["count"].clone()),
        (5125.into(), 36.into())
    );
    assert!(json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"].is_null());
    assert_eq!(json["meshes"][0]["primitives"][0]["mode"], 4);
    let (positions, triangles) = mesh_data(&loaded, 0);
    assert_eq!(triangles.len(), 12);
    assert!(triangles.iter().flatten().all(|i| *i < 8));
    for corner in 0..8u32 {
        let expected = [0, 1, 2].map(|a| {
            if corner >> a & 1 == 1 {
                0.005f32
            } else {
                -0.005f32
            }
        });
        assert!(positions.contains(&expected), "missing corner {expected:?}");
    }
    assert_outward(&positions, &triangles);
    assert_eq!(json["nodes"][0]["name"], "pCube1");
    assert_eq!(json["scenes"][0]["nodes"][0], 0);
}

#[test]
fn glb_container_layout_and_payload_match_the_gltf_export() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let (_, gltf) = export(&scene, "cube.gltf");
    let (_, glb) = export(&scene, "cube.glb");
    let embedded = load_gltf(&gltf);
    let bytes = fs::read(glb).unwrap();
    assert_eq!(&bytes[0..4], b"glTF");
    assert_eq!(u32_at(&bytes, 4), 2);
    assert_eq!(u32_at(&bytes, 8) as usize, bytes.len());
    let json_length = u32_at(&bytes, 12) as usize;
    assert_eq!(&bytes[16..20], b"JSON");
    assert_eq!(json_length % 4, 0);
    let json_end = 20 + json_length;
    let text = std::str::from_utf8(&bytes[20..json_end]).unwrap();
    let json: Value = serde_json::from_str(text).unwrap();
    assert!(json["buffers"][0]["uri"].is_null());
    assert_eq!(json["buffers"][0]["byteLength"], 240);
    let bin_length = u32_at(&bytes, json_end) as usize;
    assert_eq!(&bytes[json_end + 4..json_end + 8], b"BIN\0");
    assert_eq!(json_end + 8 + bin_length, bytes.len());
    assert_eq!(&bytes[json_end + 8..], embedded.buffer.as_slice());
}

#[test]
fn json_chunk_is_space_padded_to_four_bytes() {
    // Vary the name length so at least one of four consecutive lengths needs padding.
    let mut padded = 0;
    for name in ["a", "ab", "abc", "abcd"] {
        let scene = scene_with(&[(Primitive::Cube, name)]);
        let (_, glb) = export(&scene, "x.glb");
        let bytes = fs::read(glb).unwrap();
        let length = u32_at(&bytes, 12) as usize;
        assert_eq!(length % 4, 0);
        let chunk = &bytes[20..20 + length];
        let trimmed = chunk.trim_ascii_end();
        assert_eq!(trimmed.last(), Some(&b'}'));
        padded += usize::from(trimmed.len() != chunk.len());
        serde_json::from_slice::<Value>(chunk).unwrap();
    }
    assert!(padded > 0);
}

#[test]
fn positions_are_f64_centimeters_divided_once_then_rounded_to_f32() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let id = scene.nodes()[0].id();
    scene
        .set_channel(id, Channel::TranslateX, 16_777_219.125)
        .unwrap();
    let (_, path) = export(&scene, "far.gltf");
    let loaded = load_gltf(&path);
    // World x is 16777218.625 or 16777219.625 cm, exact in f64 and not in f32; meters are
    // 167772.18625 and 167772.19625, each rounded once.
    let (positions, _) = mesh_data(&loaded, 0);
    let xs: Vec<f32> = positions.iter().map(|p| p[0]).collect();
    assert!(xs.contains(&167_772.186_25_f32));
    assert!(xs.contains(&167_772.196_25_f32));
    assert_eq!(
        as_f32(&loaded.json["accessors"][0]["max"][0]),
        167_772.196_25_f32
    );
}

#[test]
fn mirrored_instance_keeps_faces_outward() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let id = scene.nodes()[0].id();
    scene.set_channel(id, Channel::ScaleX, -2.0).unwrap();
    let (_, path) = export(&scene, "mirror.gltf");
    let loaded = load_gltf(&path);
    let (positions, triangles) = mesh_data(&loaded, 0);
    assert!(positions.iter().all(|p| p[0].abs() == 0.01));
    assert_outward(&positions, &triangles);
}

#[test]
fn second_mesh_sections_follow_the_first_with_local_indices() {
    let scene = scene_with(&[(Primitive::Cube, "first"), (Primitive::Plane, "second")]);
    let (summary, path) = export(&scene, "two.gltf");
    // Plane: 11x11 vertices, 10x10 quads, two triangles each.
    assert_eq!(
        summary.unwrap(),
        GltfSummary {
            objects: 2,
            vertices: 8 + 121,
            triangles: 12 + 200
        }
    );
    let loaded = load_gltf(&path);
    let views = &loaded.json["bufferViews"];
    assert_eq!(views[2]["byteOffset"], 240);
    assert_eq!(views[2]["byteLength"], 121 * 12);
    assert_eq!(views[3]["byteOffset"], 240 + 121 * 12);
    assert_eq!(
        loaded.json["buffers"][0]["byteLength"],
        240 + 121 * 12 + 200 * 12
    );
    let (positions, triangles) = mesh_data(&loaded, 1);
    assert_eq!(positions.len(), 121);
    assert_eq!(triangles.iter().flatten().max(), Some(&120));
    assert_eq!(loaded.json["nodes"][1]["mesh"], 1);
    assert_eq!(loaded.json["nodes"][1]["name"], "second");
}

#[test]
fn refusals_write_nothing_and_name_the_reason() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let all: Vec<&ViewInstance> = view.instances().iter().collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("a.gltf");
    assert!(matches!(
        export_native_new(&directory.path().join("a.obj"), &scene, &all, &|| true),
        Err(GltfError::UnknownExtension)
    ));
    assert!(matches!(
        export_native_new(&path, &scene, &[], &|| true),
        Err(GltfError::NothingToExport)
    ));
    assert!(matches!(
        export_native_new(&path, &scene, &all, &|| false),
        Err(GltfError::Cancelled)
    ));
    assert!(!path.exists());
    fs::write(&path, b"keep").unwrap();
    match export_native_new(&path, &scene, &all, &|| true) {
        Err(GltfError::Io { source, .. }) => {
            assert_eq!(source.kind(), std::io::ErrorKind::AlreadyExists)
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), b"keep");
}

#[test]
fn assigned_session_material_is_refused_but_the_default_group_is_not() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    assert!(export(&scene, "plain.gltf").0.is_ok());
    let shape = scene.shapes()[0].id();
    scene
        .assign_session_material(
            shape,
            MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        )
        .unwrap();
    assert!(matches!(
        export(&scene, "red.gltf").0,
        Err(GltfError::SessionMaterial)
    ));
}

#[test]
fn session_uvs_are_refused_not_dropped() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    let (result, path) = export(&scene, "uv.gltf");
    assert!(matches!(result, Err(GltfError::SessionUvs)));
    assert!(!path.exists());
}
