mod structure {
    include!("support/maya_ascii_structure.rs");
}
use structure::validate;

const GOLDEN: &str = include_str!("snapshots/maya_ascii_cube_golden.ma");
const HEADER: &str = "requires maya \"2026\";\n\
    currentUnit -l centimeter -a degree -t film;\n\
    fileInfo \"AztecCraft\" \"1\";\n";
const PLANE_BODY: &str = "createNode transform -n \"p\";\n\
    createNode mesh -n \"pShape\" -p \"p\";\n\
    setAttr -s 4 \".vt[0:3]\" 0 0 0 1 0 0 1 0 1 0 0 1;\n\
    setAttr -s 4 \".ed[0:3]\" 0 1 0 1 2 0 2 3 0 3 0 0;\n";
const PLANE_FACE: &str = "setAttr -s 1 \".fc[0:0]\" -type \"polyFaces\" f 4 0 1 2 3;\n";

fn plane() -> String {
    format!("{HEADER}{PLANE_BODY}{PLANE_FACE}")
}

fn rejects(text: &str, needle: &str, label: &str) {
    match validate(text) {
        Err(error) => assert!(
            error.contains(needle),
            "{label}: {error:?} lacks {needle:?}"
        ),
        Ok(_) => panic!("{label}: accepted a file that must be rejected"),
    }
}

fn mutated(from: &str, to: &str) -> String {
    assert_eq!(GOLDEN.matches(from).count(), 1, "{from}");
    GOLDEN.replacen(from, to, 1)
}

#[test]
fn golden_decodes_to_the_hand_traced_cube() {
    let scene = validate(GOLDEN).unwrap();
    let mesh = &scene["cubeShape"];
    let expected = [
        [0, 1, 3, 2],
        [2, 3, 5, 4],
        [4, 5, 7, 6],
        [6, 7, 1, 0],
        [1, 7, 5, 3],
        [6, 0, 2, 4],
    ];
    assert_eq!(mesh.loops().unwrap(), expected);
    assert_eq!(mesh.edges[0], [0, 1]);
    assert_eq!(mesh.edges[11], [7, 1]);
    assert_eq!(mesh.points[3], [1.0, 2.0, 3.0]);
    assert_eq!(mesh.faces[3], [3, 11, -1, -11]);
    mesh.check(true).unwrap();
    assert!((mesh.signed_volume().unwrap() - 48.0).abs() < 1e-9);
}

#[test]
fn open_plane_uses_each_edge_once() {
    let scene = validate(&plane()).unwrap();
    scene["pShape"].check(false).unwrap();
    assert!(scene["pShape"].check(true).unwrap_err().contains("closed"));
}

#[test]
fn inward_winding_has_negative_volume() {
    let mirrored = mutated(
        "-1 -2 3 1 -2 3 -1 2 3 1 2 3 -1 2 -3 1 2 -3 -1 -2 -3 1 -2 -3",
        "-1 -2 -3 1 -2 -3 -1 2 -3 1 2 -3 -1 2 3 1 2 3 -1 -2 3 1 -2 3",
    );
    let scene = validate(&mirrored).unwrap();
    assert!(scene["cubeShape"].signed_volume().unwrap() < 0.0);
}

fn appended(line: &str) -> String {
    format!("{GOLDEN}{line}\n")
}

#[test]
fn rejects_broken_topology() {
    let cases = [
        (
            "flipped sign",
            mutated("3 11 -1 -11", "3 11 1 -11"),
            "does not close",
        ),
        (
            "open loop",
            mutated("f 4 10 4 6 8", "f 3 10 4 6"),
            "does not close",
        ),
        (
            "edge used twice",
            mutated("f 4 1 7 -3 -7", "f 4 0 5 -2 -5"),
            "used twice",
        ),
        (
            "two-edge face",
            mutated("f 4 10 4 6 8", "f 2 0 -1"),
            "fewer than 3",
        ),
        (
            "self-loop edge",
            mutated("6 0 0 7 1 0", "6 0 0 7 7 0"),
            "self-loop",
        ),
        (
            "smooth flag",
            mutated("6 0 0 7 1 0", "6 0 0 7 1 2"),
            "smooth flag",
        ),
        (
            "edge endpoint",
            mutated("6 0 0 7 1 0", "6 0 0 7 8 0"),
            "endpoint",
        ),
        (
            "face edge range",
            mutated("f 4 10 4 6 8", "f 4 10 4 6 12"),
            "out of range",
        ),
        (
            "i64::MIN token",
            mutated("f 4 10 4 6 8", "f 4 10 4 6 -9223372036854775808"),
            "out of range",
        ),
        (
            "short face record",
            mutated("f 4 10 4 6 8", "f 5 10 4 6 8"),
            "count mismatch",
        ),
        (
            "unknown record",
            mutated("f 4 10 4 6 8", "g 4 10 4 6 8"),
            "polyFaces record",
        ),
        (
            "fc size",
            mutated("-s 6 \".fc[0:5]\"", "-s 5 \".fc[0:4]\""),
            "fc size",
        ),
        (
            "vt -s",
            mutated("-s 8 \".vt[0:7]\"", "-s 9 \".vt[0:7]\""),
            "range",
        ),
        (
            "ed range",
            mutated("\".ed[0:11]\"", "\".ed[0:10]\""),
            "range",
        ),
        ("miscounted values", mutated("7 -3 -7", "7 -3"), "face id"),
        ("non-finite", mutated("0.3333333333333333", "nan"), "finite"),
    ];
    for (label, text, needle) in &cases {
        rejects(text, needle, label);
    }
}

#[test]
fn rejects_broken_structure() {
    let uid = "6C1B2D4E-0A3F-4B5C-8D7E-9F0A1B2C3D4E";
    let cases = [
        (
            "forward reference",
            mutated("-p \"cube\"", "-p \"red\""),
            "not created yet",
        ),
        (
            "mesh without parent",
            mutated(" -p \"cube\"", ""),
            "needs a transform parent",
        ),
        (
            "lambert under shape",
            mutated("-n \"red\"", "-n \"red\" -p \"cubeShape\""),
            "must not have parent",
        ),
        (
            "duplicate name",
            mutated("-n \"red\"", "-n \"cube\""),
            "duplicate",
        ),
        ("set twice", mutated("\".s\"", "\".t\""), "set twice"),
        (
            "two channel values",
            mutated("double3\" 1 1 1", "double3\" 1 1"),
            "unsupported attribute",
        ),
        (
            "createNode after connect",
            appended("createNode transform -n \"late\";"),
            "after connectAttr",
        ),
        (
            "unknown endpoint",
            appended("connectAttr \"ghost.oc\" \"redSG.ss\";"),
            "unknown node",
        ),
        (
            "dsm without -na",
            mutated("\"redSG.dsm\" -na", "\"redSG.dsm\""),
            "unsupported connection",
        ),
        (
            "oc with trailing flag",
            mutated("\"redSG.ss\"", "\"redSG.ss\" -na"),
            "unsupported connection",
        ),
        ("uid too short", mutated(uid, &uid[1..]), "bad rename"),
        (
            "uid not hex",
            mutated(uid, &uid.replace('6', "G")),
            "bad rename",
        ),
        (
            "uid grouping",
            mutated(uid, "6C1B2D4E0-A3F-4B5C-8D7E-9F0A1B2C3D4E"),
            "bad rename",
        ),
        (
            "setAttr first",
            format!("{HEADER}setAttr \".b\" 1;"),
            "before any createNode",
        ),
        (
            "mesh lacks fc",
            format!("{HEADER}{PLANE_BODY}"),
            "lacks vt, ed or fc",
        ),
        (
            "mesh under lambert",
            format!(
                "{HEADER}createNode lambert -n \"p\";{}",
                &PLANE_BODY[PLANE_BODY.find('\n').unwrap()..]
            ),
            "needs a transform parent",
        ),
    ];
    for (label, text, needle) in &cases {
        rejects(text, needle, label);
    }
}

#[test]
fn rejects_wrong_headers() {
    let (requires, unit, info) = (
        HEADER.lines().next().unwrap(),
        HEADER.lines().nth(1).unwrap(),
        HEADER.lines().nth(2).unwrap(),
    );
    let swapped = format!("{unit}\n{requires}\n{info}\n{}", PLANE_BODY);
    let marker_late = format!("{requires}\n{unit}\ncreateNode transform -n \"x\";\n{info}\n");
    let cases = [
        ("no header", format!("{PLANE_BODY}{PLANE_FACE}"), "header"),
        ("swapped header", format!("{swapped}{PLANE_FACE}"), "header"),
        ("marker not third", marker_late, "header"),
        (
            "unknown version",
            plane().replace("\"AztecCraft\" \"1\"", "\"AztecCraft\" \"2\""),
            "header",
        ),
        (
            "wrong requires",
            plane().replace("requires maya", "requires mel"),
            "header",
        ),
        ("units", plane().replace("centimeter", "meter"), "header"),
        (
            "duplicate marker",
            plane().replacen(PLANE_BODY, &format!("{info}\n{PLANE_BODY}"), 1),
            "whitelist",
        ),
    ];
    for (label, text, needle) in &cases {
        rejects(text, needle, label);
    }
}

#[test]
fn rejects_scripts_and_malformed_text() {
    let info = "fileInfo \"AztecCraft\" \"1\"";
    let cases = [
        (
            "python",
            mutated("-n \"red\"", "-n \"red\"; python \"x\""),
            "script-capable",
        ),
        (
            "eval",
            mutated("-n \"red\"", "-n \"red\"; eval \"x\""),
            "script-capable",
        ),
        (
            "scriptNode",
            appended("createNode scriptNode -n \"s\";"),
            "script-capable",
        ),
        (
            "-command",
            mutated("-n \"red\"", "-n \"red\" -command"),
            "script-capable",
        ),
        (
            "script type",
            mutated(
                "createNode shadingEngine",
                "createNode script -n \"s\";\ncreateNode shadingEngine",
            ),
            "unsupported or duplicate",
        ),
        (
            "unknown command",
            mutated("-n \"red\"", "-n \"red\"; source \"x\""),
            "whitelist",
        ),
        ("later fileInfo", appended(&format!("{info};")), "whitelist"),
        (
            "missing semicolon",
            GOLDEN.trim_end().trim_end_matches(';').into(),
            "terminated",
        ),
        (
            "open string",
            mutated("\"red.oc\"", "\"red.oc"),
            "unterminated",
        ),
    ];
    for (label, text, needle) in &cases {
        rejects(text, needle, label);
    }
}
