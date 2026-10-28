//! Records, without gating anything, that MayaSceneKit (pinned commit 95a11db) decodes the
//! Maya-written `dummy_mesh.mb` but not the writer's meshes (inferred cause: the empty UV set). Maya acceptance of these files is unproven; this is the only third-party
//! reader the project can run. It needs a locally built CLI, never builds one, and is ignored by
//! default. How to build and run it: `.project-docs/mayascenekit-crosscheck.md`.
use super::*;
use crate::scene::Primitive;
use serde_json::Value;
use std::process::Command;

const ENV: &str = "AZTECCRAFT_MSK_BIN";

/// The one issue the writer's cube produces at the pinned commit: no decoder for the `MESH` leaf.
fn is_undecoded_mesh(issue: &Value) -> bool {
    let field = |key: &str| issue.get(key).and_then(Value::as_str);
    field("trace_tag") == Some("MESH")
        && field("trace_form") == Some("DMSH")
        && field("kind") == Some("unsupported")
}

/// Runs the forensic `to-ascii` command on `input` and returns the parsed issues JSON.
fn forensic_report(binary: &std::ffi::OsStr, input: &std::path::Path) -> Value {
    let directory = tempfile::tempdir().unwrap();
    let (output, report) = (
        directory.path().join("out.ma"),
        directory.path().join("issues.json"),
    );
    let run = Command::new(binary)
        .arg("to-ascii")
        .arg(input)
        .arg(&output)
        .arg("--issues-json")
        .arg(&report)
        .args(["--mode", "forensic"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&run.stderr) + String::from_utf8_lossy(&run.stdout);
    assert!(
        run.status.success(),
        "MayaSceneKit failed in forensic mode: {text}"
    );
    let bytes = std::fs::read(&report)
        .unwrap_or_else(|error| panic!("MayaSceneKit wrote no issues JSON ({error}): {text}"));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("MayaSceneKit issues JSON is malformed ({error})"))
}

fn issue_list(report: &Value) -> &Vec<Value> {
    report
        .get("issues")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("MayaSceneKit issues JSON has no `issues` array: {report}"))
}

fn has_exact_mesh_decode(report: &Value) -> bool {
    report
        .get("decode_quality_distribution")
        .and_then(Value::as_array)
        .is_some_and(|entries| {
            entries.iter().any(|entry| {
                let field = |key: &str| entry.get(key).and_then(Value::as_str);
                field("form") == Some("DMSH")
                    && field("tag") == Some("MESH")
                    && field("quality") == Some("exact")
            })
        })
}

#[test]
#[ignore = "needs AZTECCRAFT_MSK_BIN, a MayaSceneKit CLI built from commit 95a11db"]
fn msk_decodes_maya_meshes_but_not_the_writers_mesh() {
    let Some(binary) = std::env::var_os(ENV) else {
        eprintln!("{ENV} is unset: skipping the MayaSceneKit cross-check");
        return;
    };
    let control_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gt-tools/dummy_mesh.mb");
    let control = forensic_report(&binary, &control_path);
    assert!(
        !issue_list(&control)
            .iter()
            .any(|issue| issue.get("trace_tag").and_then(Value::as_str) == Some("MESH")),
        "MayaSceneKit now reports a MESH issue for the Maya-written control: {control}"
    );
    assert!(
        has_exact_mesh_decode(&control),
        "MayaSceneKit no longer lists an exact DMSH/MESH decode for the control: {control}"
    );

    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube1").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("cube.mb");
    std::fs::write(&input, encode_authored(&scene).unwrap()).unwrap();
    let issues = forensic_report(&binary, &input);
    assert!(
        issue_list(&issues).iter().any(is_undecoded_mesh),
        "MayaSceneKit reports no unsupported MESH/DMSH issue any more: it may now decode our \
         meshes, or the report format changed. Revisit known-issues.md (Mesh Save is \
         PROVISIONAL) and this test. Report: {issues}"
    );
}
