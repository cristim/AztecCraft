//! Records, without gating anything, that MayaSceneKit (pinned commit 95a11db) does not accept
//! the writer's meshes. Maya acceptance of these files is unproven; this is the only third-party
//! reader the project can run. It needs a locally built CLI, never builds one, and is ignored by
//! default. How to build and run it: `.project-docs/mayascenekit-crosscheck.md`.
use super::*;
use crate::scene::Primitive;
use serde_json::Value;
use std::process::Command;

const ENV: &str = "AZTECCRAFT_MSK_BIN";

/// An issue or message points at the mesh chunks (`DMSH` group, `MESH` leaf).
fn names_the_mesh(text: &str) -> bool {
    text.contains("MESH") || text.contains("DMSH")
}

fn mesh_issues(issues: &Value) -> Vec<&Value> {
    issues
        .get("issues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|issue| {
            ["trace_tag", "trace_form"].iter().any(|key| {
                issue
                    .get(key)
                    .and_then(Value::as_str)
                    .is_some_and(names_the_mesh)
            })
        })
        .collect()
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
        .output()
        .unwrap();
    if !run.status.success() {
        // A hard refusal: it must still name the mesh chunk, or the reason has changed.
        let text = String::from_utf8_lossy(&run.stderr) + String::from_utf8_lossy(&run.stdout);
        assert!(
            names_the_mesh(&text),
            "MayaSceneKit failed for another reason: {text}"
        );
        return;
    }
    let issues: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
    assert!(
        !mesh_issues(&issues).is_empty(),
        "MayaSceneKit reports no issue on the DMSH/MESH chunk any more: it may now decode these \
         meshes. Revisit known-issues.md (Mesh Save is PROVISIONAL) and this test. Report: {issues}"
    );
}
