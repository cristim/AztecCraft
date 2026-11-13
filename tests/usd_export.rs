//! USD ASCII export oracle. Expected text and values are computed by hand from the OpenUSD
//! `UsdGeomMesh` schema (counts, indices, point3f points, right-handed counter-clockwise faces,
//! `metersPerUnit`/`upAxis` layer metadata), and the output is read back by the minimal usda
//! reader below, which shares no code with the exporter.

#![allow(clippy::excessive_precision)]

use azteccraft::{
    native_scene::NativeScene,
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
