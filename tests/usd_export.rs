//! USD ASCII export oracle. Expected text and values are computed by hand from the OpenUSD
//! `UsdGeomMesh` schema (counts, indices, point3f points, right-handed counter-clockwise faces,
//! `metersPerUnit`/`upAxis` layer metadata), and the output is read back by the minimal usda
//! reader below, which shares no code with the exporter.

#![allow(clippy::excessive_precision)]

use azteccraft::{
    native_scene::{Channel, MaterialChoice, NativeScene, UvProjectAxis},
    scene::Primitive,
    usd::{UsdError, UsdSummary, export_native_new},
    view_scene::{ViewInstance, ViewScene},
};
use std::{collections::HashMap, fs, path::PathBuf};

const BUDGET: usize = 64 * 1024 * 1024;

fn scene_with(primitives: &[(Primitive, &str)]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (primitive, name) in primitives {
        scene.create(*primitive, name).unwrap();
    }
    scene
}

fn export(scene: &NativeScene, file: &str) -> (Result<UsdSummary, UsdError>, PathBuf) {
    let view = ViewScene::from_native(scene, BUDGET).unwrap();
    let instances: Vec<&ViewInstance> = view.instances().iter().collect();
    let directory = Box::leak(Box::new(tempfile::tempdir().unwrap()));
    let path = directory.path().join(file);
    (export_native_new(&path, scene, &instances, &|| true), path)
}

// ---- minimal usda reader: the subset the exporter writes ----

#[derive(Debug, PartialEq)]
enum Value {
    Str(String),
    Num(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
}

#[derive(Default)]
struct Prim {
    kind: String,
    name: String,
    attrs: HashMap<String, Value>,
    children: Vec<Prim>,
}

struct Reader {
    tokens: Vec<String>,
    at: usize,
}

fn tokenize(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let (mut tokens, mut i) = (vec![], 0);
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if "()[]{}=,".contains(c) {
            tokens.push(c.to_string());
            i += 1;
        } else if c == '"' {
            let end = chars[i + 1..].iter().position(|&d| d == '"').unwrap() + i + 1;
            tokens.push(chars[i..=end].iter().collect());
            i = end + 1;
        } else {
            let end = chars[i..]
                .iter()
                .position(|&d| d.is_whitespace() || "()[]{}=,\"".contains(d))
                .map_or(chars.len(), |p| p + i);
            tokens.push(chars[i..end].iter().collect());
            i = end;
        }
    }
    tokens
}

impl Reader {
    fn next(&mut self) -> String {
        self.at += 1;
        self.tokens[self.at - 1].clone()
    }
    fn peek(&self) -> Option<&str> {
        self.tokens.get(self.at).map(String::as_str)
    }
    fn expect(&mut self, want: &str) {
        assert_eq!(self.next(), want);
    }
    fn value(&mut self) -> Value {
        let token = self.next();
        match token.as_str() {
            "[" | "(" => {
                let close = if token == "[" { "]" } else { ")" };
                let mut items = vec![];
                while self.peek() != Some(close) {
                    items.push(self.value());
                    if self.peek() != Some(close) {
                        self.expect(",");
                        assert_ne!(self.peek(), Some(close), "trailing comma");
                    }
                }
                self.next();
                if token == "[" {
                    Value::List(items)
                } else {
                    Value::Tuple(items)
                }
            }
            t if t.starts_with('"') => Value::Str(t.trim_matches('"').to_owned()),
            t => Value::Num(t.to_owned()),
        }
    }
    fn prim(&mut self) -> Prim {
        self.expect("def");
        let kind = self.next();
        let name = self.next().trim_matches('"').to_owned();
        self.expect("{");
        let mut prim = Prim {
            kind,
            name,
            ..Prim::default()
        };
        while self.peek() != Some("}") {
            if self.peek() == Some("def") {
                prim.children.push(self.prim());
                continue;
            }
            let mut ty = self.next();
            if ty == "uniform" {
                ty = self.next();
            }
            if self.peek() == Some("[") {
                self.next();
                self.expect("]");
            }
            let attr = self.next();
            self.expect("=");
            let _ = ty;
            let value = self.value();
            prim.attrs.insert(attr, value);
        }
        self.next();
        prim
    }
}

struct Layer {
    meta: HashMap<String, Value>,
    root: Prim,
}

fn read(text: &str) -> Layer {
    assert!(text.starts_with("#usda 1.0\n"));
    let mut reader = Reader {
        tokens: tokenize(&text["#usda 1.0\n".len()..]),
        at: 0,
    };
    reader.expect("(");
    let mut meta = HashMap::new();
    while reader.peek() != Some(")") {
        let key = reader.next();
        reader.expect("=");
        meta.insert(key, reader.value());
    }
    reader.next();
    let root = reader.prim();
    assert_eq!(reader.at, reader.tokens.len(), "trailing tokens");
    Layer { meta, root }
}

fn numbers(value: &Value) -> Vec<&str> {
    match value {
        Value::List(items) | Value::Tuple(items) => items
            .iter()
            .flat_map(|item| match item {
                Value::Num(n) => vec![n.as_str()],
                other => numbers(other),
            })
            .collect(),
        other => panic!("{other:?}"),
    }
}

fn ints(value: &Value) -> Vec<usize> {
    numbers(value).iter().map(|n| n.parse().unwrap()).collect()
}

fn points(value: &Value) -> Vec<[f32; 3]> {
    let flat: Vec<f32> = numbers(value).iter().map(|n| n.parse().unwrap()).collect();
    flat.chunks(3).map(|c| [c[0], c[1], c[2]]).collect()
}

/// Faces as lists of point indices, split by `faceVertexCounts`.
fn faces(mesh: &Prim) -> Vec<Vec<usize>> {
    let counts = ints(&mesh.attrs["faceVertexCounts"]);
    let indices = ints(&mesh.attrs["faceVertexIndices"]);
    assert_eq!(counts.iter().sum::<usize>(), indices.len());
    let mut at = 0;
    counts
        .iter()
        .map(|&n| {
            at += n;
            indices[at - n..at].to_vec()
        })
        .collect()
}

/// Newell normal of a face computed from the parsed points (counter-clockwise = outward).
fn assert_outward(mesh: &Prim) {
    let pts = points(&mesh.attrs["points"]);
    let centre: [f32; 3] =
        std::array::from_fn(|a| pts.iter().map(|p| p[a]).sum::<f32>() / pts.len() as f32);
    for face in faces(mesh) {
        let mut n = [0.0f32; 3];
        for k in 0..face.len() {
            let (p, q) = (pts[face[k]], pts[face[(k + 1) % face.len()]]);
            n[0] += (p[1] - q[1]) * (p[2] + q[2]);
            n[1] += (p[2] - q[2]) * (p[0] + q[0]);
            n[2] += (p[0] - q[0]) * (p[1] + q[1]);
        }
        let out: f32 = (0..3).map(|a| n[a] * (pts[face[0]][a] - centre[a])).sum();
        assert!(out > 0.0, "face {face:?} faces inward");
    }
}

const CUBE_TEXT: &str = "#usda 1.0
(
    defaultPrim = \"World\"
    metersPerUnit = 0.01
    upAxis = \"Y\"
)

def Xform \"World\"
{
    def Mesh \"pCube1\"
    {
        int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
        int[] faceVertexIndices = [0, 3, 2, 1, 4, 5, 6, 7, 0, 4, 7, 3, 1, 2, 6, 5, 0, 1, 5, 4, 3, 7, 6, 2]
        float3[] extent = [(-0.5, -0.5, -0.5), (0.5, 0.5, 0.5)]
        point3f[] points = [(-0.5, -0.5, -0.5), (0.5, -0.5, -0.5), (0.5, 0.5, -0.5), (-0.5, 0.5, -0.5), (-0.5, -0.5, 0.5), (0.5, -0.5, 0.5), (0.5, 0.5, 0.5), (-0.5, 0.5, 0.5)]
        uniform token subdivisionScheme = \"none\"
    }
}
";

#[test]
fn unit_cube_text_matches_the_hand_written_layer() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let (summary, path) = export(&scene, "cube.usda");
    assert_eq!(
        summary.unwrap(),
        UsdSummary {
            objects: 1,
            vertices: 8,
            faces: 6
        }
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), CUBE_TEXT);
}

#[test]
fn independent_reader_recovers_layer_metadata_counts_and_winding() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let (_, path) = export(&scene, "cube.USDA");
    let layer = read(&fs::read_to_string(path).unwrap());
    assert_eq!(layer.meta["defaultPrim"], Value::Str("World".into()));
    assert_eq!(layer.meta["metersPerUnit"], Value::Num("0.01".into()));
    assert_eq!(layer.meta["upAxis"], Value::Str("Y".into()));
    assert_eq!(
        (layer.root.kind.as_str(), layer.root.name.as_str()),
        ("Xform", "World")
    );
    let mesh = &layer.root.children[0];
    assert_eq!((mesh.kind.as_str(), mesh.name.as_str()), ("Mesh", "pCube1"));
    let pts = points(&mesh.attrs["points"]);
    assert_eq!(pts.len(), 8);
    for corner in 0..8usize {
        let expected: [f32; 3] =
            std::array::from_fn(|a| if corner >> a & 1 == 1 { 0.5 } else { -0.5 });
        assert!(pts.contains(&expected), "missing {expected:?}");
    }
    let f = faces(mesh);
    assert_eq!(f.len(), 6);
    assert!(f.iter().flatten().all(|i| *i < 8));
    assert_outward(mesh);
}

#[test]
fn mirrored_instance_reverses_each_face_and_stays_outward() {
    // Unmirrored control: ScaleX = 2 keeps the native order.
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let id = scene.nodes()[0].id();
    scene.set_channel(id, Channel::ScaleX, 2.0).unwrap();
    let (_, path) = export(&scene, "control.usda");
    let control = read(&fs::read_to_string(path).unwrap());
    let mesh = &control.root.children[0];
    assert_eq!(faces(mesh)[0], vec![0, 3, 2, 1]);
    assert_outward(mesh);

    // ScaleX = -2 and ScaleY = 3: determinant -6 < 0, so every face is reversed.
    scene.set_channel(id, Channel::ScaleX, -2.0).unwrap();
    scene.set_channel(id, Channel::ScaleY, 3.0).unwrap();
    let (_, path) = export(&scene, "mirror.usda");
    let layer = read(&fs::read_to_string(path).unwrap());
    let mesh = &layer.root.children[0];
    let pts = points(&mesh.attrs["points"]);
    assert!(pts.iter().all(|p| p[0].abs() == 1.0 && p[1].abs() == 1.5));
    assert_eq!(
        ints(&mesh.attrs["faceVertexIndices"]),
        [
            1, 2, 3, 0, 7, 6, 5, 4, 3, 7, 4, 0, 5, 6, 2, 1, 4, 5, 1, 0, 2, 6, 7, 3
        ]
    );
    assert_outward(mesh);

    // Two negative scales: determinant +, so no reversal.
    scene.set_channel(id, Channel::ScaleX, -1.0).unwrap();
    scene.set_channel(id, Channel::ScaleY, -1.0).unwrap();
    let (_, path) = export(&scene, "double.usda");
    let layer = read(&fs::read_to_string(path).unwrap());
    assert_eq!(faces(&layer.root.children[0])[0], vec![0, 3, 2, 1]);
    assert_outward(&layer.root.children[0]);
}

#[test]
fn points_are_f32_exact_in_shortest_round_trip_text() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let id = scene.nodes()[0].id();
    scene.set_channel(id, Channel::TranslateX, 0.1).unwrap();
    scene
        .set_channel(id, Channel::TranslateY, 1234.5678)
        .unwrap();
    scene
        .set_channel(id, Channel::TranslateZ, 16_000_000.0)
        .unwrap();
    let (_, path) = export(&scene, "precise.usda");
    let text = fs::read_to_string(&path).unwrap();
    // x = 0.1 -+ 0.5 -> -0.4 and 0.6; the shortest text that reads back as the same f32.
    assert!(text.contains("(-0.4, "), "{text}");
    assert!(text.contains("(0.6, "), "{text}");
    let layer = read(&text);
    let pts = points(&layer.root.children[0].attrs["points"]);
    let expected_y = [1_234.067_8_f32, 1_235.067_8_f32];
    let expected_z = [16_000_000.0_f32];
    for p in &pts {
        assert!(p[0] == -0.4f32 || p[0] == 0.6f32);
        assert!(
            expected_y.iter().any(|y| y.to_bits() == p[1].to_bits()),
            "{p:?}"
        );
        assert!(
            expected_z.iter().any(|z| z.to_bits() == p[2].to_bits()),
            "{p:?}"
        );
    }
    let extent = points(&layer.root.children[0].attrs["extent"]);
    assert_eq!(extent[0][0], -0.4f32);
    assert_eq!(extent[1][0], 0.6f32);
}

#[test]
fn two_meshes_keep_local_indices_and_names() {
    let scene = scene_with(&[(Primitive::Cube, "first"), (Primitive::Plane, "second")]);
    let (summary, path) = export(&scene, "two.usda");
    // Plane: 11x11 vertices, 10x10 quads.
    assert_eq!(
        summary.unwrap(),
        UsdSummary {
            objects: 2,
            vertices: 8 + 121,
            faces: 6 + 100
        }
    );
    let layer = read(&fs::read_to_string(path).unwrap());
    let names: Vec<_> = layer
        .root
        .children
        .iter()
        .map(|m| m.name.as_str())
        .collect();
    assert_eq!(names, ["first", "second"]);
    let plane = &layer.root.children[1];
    assert_eq!(points(&plane.attrs["points"]).len(), 121);
    assert_eq!(
        ints(&plane.attrs["faceVertexIndices"]).iter().max(),
        Some(&120)
    );
    assert!(
        ints(&plane.attrs["faceVertexCounts"])
            .iter()
            .all(|c| *c == 4)
    );
}

#[test]
fn refusals_write_nothing_and_name_the_reason() {
    let scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let view = ViewScene::from_native(&scene, BUDGET).unwrap();
    let all: Vec<&ViewInstance> = view.instances().iter().collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("a.usda");
    for bad in ["a.obj", "a.usd", "a.USD", "a.usdc", "a.usdz", "a"] {
        assert!(matches!(
            export_native_new(&directory.path().join(bad), &scene, &all, &|| true),
            Err(UsdError::UnknownExtension)
        ));
    }
    assert!(matches!(
        export_native_new(&path, &scene, &[], &|| true),
        Err(UsdError::NothingToExport)
    ));
    assert!(matches!(
        export_native_new(&path, &scene, &all, &|| false),
        Err(UsdError::Cancelled)
    ));
    assert!(!path.exists());
    fs::write(&path, b"keep").unwrap();
    match export_native_new(&path, &scene, &all, &|| true) {
        Err(UsdError::Io { source, .. }) => {
            assert_eq!(source.kind(), std::io::ErrorKind::AlreadyExists)
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(fs::read(&path).unwrap(), b"keep");
}

#[test]
fn session_material_and_uvs_are_refused_not_dropped() {
    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    assert!(export(&scene, "plain.usda").0.is_ok());
    let shape = scene.shapes()[0].id();
    scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    let (result, path) = export(&scene, "uv.usda");
    assert!(matches!(result, Err(UsdError::SessionUvs)));
    assert!(!path.exists());

    let mut scene = scene_with(&[(Primitive::Cube, "pCube1")]);
    let shape = scene.shapes()[0].id();
    scene
        .assign_session_material(
            shape,
            MaterialChoice::New {
                color: [1.0, 0.0, 0.0],
            },
        )
        .unwrap();
    let (result, path) = export(&scene, "red.usda");
    assert!(matches!(result, Err(UsdError::SessionMaterial)));
    assert!(!path.exists());
}
