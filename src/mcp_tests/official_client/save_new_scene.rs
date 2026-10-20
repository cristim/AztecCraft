//! Save As for a new (Untitled) scene over the official MCP client. Test A is the maintainer's
//! acceptance flow and stays one uninterrupted sequence; the extras are separate tests.
use super::*;

type Client = rmcp::service::RunningService<RoleClient, ()>;

/// Calls `name` with the live scene token and returns the structured result.
async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &Client,
    name: &'static str,
    mut arguments: Value,
) -> Value {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments)
        .await
        .structured_content
        .unwrap()
}

async fn inspect_json(editor: &mut Editor, context: &egui::Context, client: &Client) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn vec3(value: &Value) -> [f64; 3] {
    let items = value.as_array().unwrap();
    [0, 1, 2].map(|i| items[i].as_f64().unwrap())
}

/// Names, transforms and mesh counts in node order: what a reopen must reproduce.
#[derive(Debug)]
struct NodeSummary {
    name: String,
    /// Translation, rotation in degrees and scale.
    channels: [[f64; 3]; 3],
    shape: String,
    /// Points, edges and faces.
    counts: [u64; 3],
}

fn summary(inspection: &Value) -> Vec<NodeSummary> {
    let shapes = inspection["shapes"].as_array().unwrap();
    inspection["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(shapes)
        .map(|(node, shape)| {
            let transform = &node["transform"];
            NodeSummary {
                name: node["name"].as_str().unwrap().to_owned(),
                channels: [
                    vec3(&transform["translation"]),
                    vec3(&transform["rotation_degrees"]),
                    vec3(&transform["scale"]),
                ],
                shape: shape["name"].as_str().unwrap().to_owned(),
                counts: ["points", "edges", "faces"].map(|k| shape[k].as_u64().unwrap()),
            }
        })
        .collect()
}

#[cfg(unix)]
#[tokio::test]
async fn native_sdk_new_scene_save_as_reopen_inspect_and_move_succeeds() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let destination = directory.path().join("authored.mb");
    for primitive in ["cube", "sphere", "cylinder", "cone"] {
        let created = tool(
            &mut editor,
            &context,
            &client,
            "create_primitive",
            json!({"primitive": primitive}),
        )
        .await;
        assert_eq!(created["outcome"], "created");
    }
    let before = inspect_json(&mut editor, &context, &client).await;
    let second = before["nodes"][1]["id"].as_str().unwrap().to_owned();
    let edited = tool(
        &mut editor,
        &context,
        &client,
        "set_transform",
        json!({"node_id": second, "translation": [1.5, -2.0, 3.25],
               "rotation_degrees": [10.0, 20.0, 30.0], "scale": [1.0, 2.0, 0.5]}),
    )
    .await;
    assert_eq!(edited["outcome"], "changed");
    let before = inspect_json(&mut editor, &context, &client).await;
    assert_eq!(before["document_origin"], "untitled");
    assert_eq!(before["dirty"], true);

    // The acceptance flow: save_scene_as, open it again, inspect, move. Nothing in between.
    let saved = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path": destination}),
    )
    .await;
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path": destination}),
    )
    .await;
    assert_eq!(opened["outcome"], "document_applied");
    let after = inspect_json(&mut editor, &context, &client).await;
    assert_eq!(after["document_origin"], "maya_binary");
    assert_eq!(after["dirty"], false);
    assert_eq!(after["has_file"], true);
    let (names_before, names_after) = (summary(&before), summary(&after));
    assert_eq!(names_after.len(), 4);
    for (expected, actual) in names_before.iter().zip(&names_after) {
        assert_eq!(
            (&expected.name, &expected.shape, &expected.counts),
            (&actual.name, &actual.shape, &actual.counts)
        );
        let pairs = expected.channels.iter().zip(&actual.channels);
        for (e, a) in pairs.flat_map(|(e, a)| e.iter().zip(a)) {
            // Rotations go degrees -> radians -> degrees, which is not bit-exact.
            assert!((e - a).abs() < 1e-9, "{expected:?} != {actual:?}");
        }
    }
    let node = &after["nodes"][0];
    let selected = tool(
        &mut editor,
        &context,
        &client,
        "select_node",
        json!({"node_id": node["id"], "instance_path": node["instance_paths"][0]}),
    )
    .await;
    assert!(selected.get("error").is_none(), "{selected}");
    let moved = tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"node_id": node["id"], "instance_path": node["instance_paths"][0],
               "kind": "move", "axis": "x"}),
    )
    .await;
    assert_eq!(moved["outcome"], "manipulation_started", "{moved}");
    let id = moved["manipulation_id"].as_str().unwrap().to_owned();
    let previewed = tool(
        &mut editor,
        &context,
        &client,
        "update_manipulation",
        json!({"manipulation_id": id, "distance": 2.5}),
    )
    .await;
    assert!(previewed.get("error").is_none(), "{previewed}");
    let finished = tool(
        &mut editor,
        &context,
        &client,
        "finish_manipulation",
        json!({"manipulation_id": id, "finish": "commit"}),
    )
    .await;
    assert!(finished.get("error").is_none(), "{finished}");
    assert_eq!(
        editor.history.scene().nodes()[0].transform().translation[0],
        2.5
    );
    assert!(editor.is_dirty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
