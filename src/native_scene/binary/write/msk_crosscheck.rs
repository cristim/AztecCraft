//! Records, without gating anything, that MayaSceneKit (pinned commit 95a11db) does not accept
//! the writer's meshes. Maya acceptance of these files is unproven; this is the only third-party
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

#[test]
#[ignore = "needs AZTECCRAFT_MSK_BIN, a MayaSceneKit CLI built from commit 95a11db"]
fn msk_refuses_the_writers_empty_uv_mesh() {
    let Some(binary) = std::env::var_os(ENV) else {
        eprintln!("{ENV} is unset: skipping the MayaSceneKit cross-check");
        return;
    };
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "cube1").unwrap();
    let directory = tempfile::tempdir().unwrap();
    let (input, output, report) = (
        directory.path().join("cube.mb"),
        directory.path().join("cube.ma"),
        directory.path().join("issues.json"),
    );
    std::fs::write(&input, encode_authored(&scene).unwrap()).unwrap();
    let run = Command::new(binary)
        .arg("to-ascii")
        .arg(&input)
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
    let issues: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|error| panic!("MayaSceneKit issues JSON is malformed ({error})"));
    let list = issues
        .get("issues")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("MayaSceneKit issues JSON has no `issues` array: {issues}"));
    assert!(
        list.iter().any(is_undecoded_mesh),
        "MayaSceneKit reports no unsupported MESH/DMSH issue any more: it may now decode our \
         meshes, or the report format changed. Revisit known-issues.md (Mesh Save is \
         PROVISIONAL) and this test. Report: {issues}"
    );
}
