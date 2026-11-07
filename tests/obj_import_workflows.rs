use azteccraft::{
    native_scene::NativeScene,
    obj::{
        ExportSummary, export_native_new,
        import::{MAX_CORNERS, MAX_FACES, MAX_VERTICES, ObjImportError, ParsedObj, parse, read},
    },
    scene::Primitive,
    view_scene::{ViewInstance, ViewScene},
};
use std::fs;

fn exported(primitives: &[(Primitive, &str)]) -> (String, ExportSummary) {
    let mut scene = NativeScene::default();
    for (primitive, name) in primitives {
        scene.create(*primitive, name).unwrap();
    }
    let view = ViewScene::from_native(&scene, 64 * 1024 * 1024).unwrap();
    let instances: Vec<&ViewInstance> = view.instances().iter().collect();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("out.obj");
    let summary = export_native_new(&path, &scene, &instances, &|| true).unwrap();
    (fs::read_to_string(&path).unwrap(), summary)
}

fn assert_sound(parsed: &ParsedObj) {
    assert!(parsed.vertices.len() <= MAX_VERTICES);
    assert!(!parsed.objects.is_empty());
    let (mut faces, mut corners) = (0, 0);
    for object in &parsed.objects {
        for face in &object.faces {
            assert!(face.len() >= 3);
            assert!(face.iter().all(|i| (*i as usize) < parsed.vertices.len()));
            let mut sorted = face.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), face.len());
            faces += 1;
            corners += face.len();
        }
    }
    assert!(faces <= MAX_FACES && corners <= MAX_CORNERS);
    assert!(parsed.vertices.iter().flatten().all(|c| c.is_finite()));
}

#[test]
fn the_exporters_output_parses_back_with_its_objects_faces_and_vertices() {
    let (text, summary) = exported(&[
        (Primitive::Cube, "pCube1"),
        (Primitive::Plane, "pPlane1"),
        (Primitive::Cube, "pCube2"),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("scene.obj");
    fs::write(&path, &text).unwrap();
    let parsed = read(&path).unwrap();
    assert_sound(&parsed);
    let names: Vec<_> = parsed.objects.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(names, ["pCube1", "pPlane1", "pCube2"]);
    let faces: Vec<_> = parsed.objects.iter().map(|o| o.faces.len()).collect();
    assert_eq!(faces, [6, 100, 6]);
    assert_eq!(parsed.objects.len(), summary.objects);
    assert_eq!(parsed.vertices.len(), summary.vertices);
    assert_eq!(faces.iter().sum::<usize>(), summary.faces);
    assert_eq!(parsed.ignored_records, 0);
}

struct Xorshift(u64);

impl Xorshift {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

const HAND: &str = "\u{feff}# hand\nmtllib a.mtl\nv 0 0 0\nv 1 0 0 1\nv 0 1 0\nv 1 1 0\nvt 0 0\nvn 0 0 1\n\
                    o one\nf 1/1/1 2/1/1 3/1/1\ng two\nusemtl m\nf -1 -2 -3 -4\nf 1 2 \\\r\n3 4\n";

fn mutate(seed: &[u8], rng: &mut Xorshift) -> Vec<u8> {
    let mut bytes = seed.to_vec();
    if bytes.is_empty() {
        return bytes;
    }
    match rng.below(5) {
        0 => {
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(bytes.len());
                bytes[at] ^= 1 << rng.below(8);
            }
        }
        1 => bytes.truncate(rng.below(bytes.len())),
        2 => {
            let lines: Vec<&[u8]> = bytes.split_inclusive(|b| *b == b'\n').collect();
            let pick = lines[rng.below(lines.len())].to_vec();
            let at = rng.below(bytes.len());
            bytes.splice(at..at, pick);
        }
        3 => {
            let numbers = [
                &b"-1"[..],
                b"0",
                b"1e309",
                b"99999999999999999999",
                b"nan",
                b"1/2/3",
                b"//",
            ];
            let at = rng.below(bytes.len());
            bytes.splice(at..at, numbers[rng.below(numbers.len())].iter().copied());
        }
        _ => {
            let at = rng.below(bytes.len());
            bytes.remove(at);
        }
    }
    bytes
}

#[test]
fn mutated_inputs_never_panic_and_every_success_is_sound() {
    let (text, _) = exported(&[(Primitive::Cube, "pCube1"), (Primitive::Plane, "pPlane1")]);
    let seeds = [text.into_bytes(), HAND.as_bytes().to_vec()];
    let mut rng = Xorshift(0x9E37_79B9_7F4A_7C15);
    let (mut accepted, mut refused) = (0, 0);
    let mut kinds = std::collections::HashSet::new();
    for seed in &seeds {
        for length in 0..seed.len().min(400) {
            let _ = parse(&seed[..length]);
        }
    }
    for round in 0..20_000 {
        let mut input = seeds[round % seeds.len()].clone();
        for _ in 0..1 + rng.below(3) {
            input = mutate(&input, &mut rng);
        }
        match parse(&input[..]) {
            Ok(parsed) => {
                assert_sound(&parsed);
                accepted += 1;
            }
            Err(error) => {
                assert!(!matches!(error, ObjImportError::Io(_)));
                assert!(!error.to_string().is_empty());
                kinds.insert(std::mem::discriminant(&error));
                refused += 1;
            }
        }
    }
    assert!(accepted > 0 && refused > 0, "{accepted} {refused}");
    assert!(
        kinds.len() >= 4,
        "only {} distinct error variants",
        kinds.len()
    );
}
