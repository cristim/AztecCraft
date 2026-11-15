//! Issue #42 over the official client: `group_selection` and `ungroup_selection` through the real
//! Editor protocol, with the results read back through `inspect_scene`.
use super::pivot_edit::{Client, code, cubes, inspected, reply, tool};
use super::*;

fn path_lengths(inspection: &Value, name: &str) -> Vec<usize> {
    inspection["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["name"] == name)
        .unwrap_or_else(|| panic!("no node {name}"))["instance_paths"]
        .as_array()
        .unwrap()
        .iter()
        .map(|path| path.as_array().unwrap().len())
        .collect()
}

async fn generation(editor: &mut Editor, context: &egui::Context, client: &Client) -> Value {
    inspected(editor, context, client).await["selection_generation"].clone()
}

#[tokio::test]
async fn group_and_ungroup_work_end_to_end_and_undo_and_show_in_inspect_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let flat = editor.history.scene().clone();
    let expected = generation(&mut editor, &context, &client).await;
    let grouped = tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"expected_selection_generation":expected}),
    )
    .await;
    let answer = reply(&grouped).clone();
    assert_eq!(answer["outcome"], "created", "{answer}");
    let group = answer["node_id"].as_str().unwrap().to_owned();
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "pCube1"), [2]);
    assert_eq!(path_lengths(&inspection, "pCube2"), [2]);
    assert_eq!(path_lengths(&inspection, "group1"), [1]);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Group"));
    assert_eq!(editor.selection.lead().unwrap().node.to_decimal(), group);

    // Undo is one step and restores the flat scene.
    let undone = tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(reply(&undone)["outcome"], "changed");
    assert_eq!(editor.history.scene(), &flat);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    // PROVISIONAL: Undo and Redo leave the selection empty, so select the group again.
    editor.select(Some(NodeId::from_decimal(&group).unwrap()));

    // The group is the selection, so Ungroup releases the cubes in order.
    let released = tool(
        &mut editor,
        &context,
        &client,
        "ungroup_selection",
        json!({}),
    )
    .await;
    let answer = reply(&released).clone();
    assert_eq!(answer["outcome"], "nodes", "{answer}");
    assert_eq!(answer["node_ids"].as_array().unwrap().len(), 2);
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "pCube1"), [1]);
    assert_eq!(editor.history.undo_label().as_deref(), Some("Ungroup"));
    assert_eq!(editor.selection.len(), 2);

    // A group inside a group ungroups one level.
    let expected = generation(&mut editor, &context, &client).await;
    tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"expected_selection_generation":expected}),
    )
    .await;
    tool(&mut editor, &context, &client, "group_selection", json!({})).await;
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "pCube1"), [3]);
    tool(
        &mut editor,
        &context,
        &client,
        "ungroup_selection",
        json!({}),
    )
    .await;
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "pCube1"), [2]);
    assert_eq!(path_lengths(&inspection, "group1"), [1]);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn under_preserve_position_and_keep_reach_the_scene() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let parent = cubes(&mut editor, 2);
    let child = editor.history.scene().nodes()[1].id();
    let translation = |editor: &Editor, id| {
        editor
            .history
            .scene()
            .node(id)
            .unwrap()
            .transform()
            .translation[0]
    };
    for (id, x) in [(parent.clone(), 10.0), (child.to_decimal(), 1.0)] {
        tool(
            &mut editor,
            &context,
            &client,
            "edit_node",
            json!({"node_id":id,"translation":[x,0,0]}),
        )
        .await;
    }
    tool(
        &mut editor,
        &context,
        &client,
        "parent_node",
        json!({"node_id":child.to_decimal(),"parent_id":parent,"keep":"local"}),
    )
    .await;
    let select_child = |editor: &mut Editor| {
        let path = editor
            .history
            .scene()
            .transform_paths()
            .unwrap()
            .into_iter()
            .find(|target| target.node == child)
            .unwrap();
        editor.set_selection_members_for_test(vec![path]);
        editor.sync_fields();
    };
    // Default: the group goes under the child's parent and nothing is rewritten.
    select_child(&mut editor);
    let made = tool(&mut editor, &context, &client, "group_selection", json!({})).await;
    assert_eq!(reply(&made)["outcome"], "created");
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "group1"), [2]);
    assert_eq!(translation(&editor, child), 1.0);
    tool(&mut editor, &context, &client, "undo", json!({})).await;

    // World keeps the world position (10 + 1); preserve_position false keeps the local value.
    select_child(&mut editor);
    tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"under":"world","pivot":"origin"}),
    )
    .await;
    let inspection = inspected(&mut editor, &context, &client).await;
    assert_eq!(path_lengths(&inspection, "group1"), [1]);
    assert_eq!(translation(&editor, child), 11.0);
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    select_child(&mut editor);
    tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"under":"world","preserve_position":false}),
    )
    .await;
    assert_eq!(translation(&editor, child), 1.0);

    // Ungroup keep local leaves the child's value (it moves in the world), keep world rewrites it.
    let group_now = editor
        .history
        .scene()
        .nodes()
        .iter()
        .find(|n| n.name() == "group1")
        .unwrap()
        .id();
    tool(
        &mut editor,
        &context,
        &client,
        "edit_node",
        json!({"node_id":group_now.to_decimal(),"translation":[100,0,0]}),
    )
    .await;
    for (keep, expected) in [("local", 1.0), ("world", 101.0)] {
        editor.select(Some(group_now));
        tool(
            &mut editor,
            &context,
            &client,
            "ungroup_selection",
            json!({"under":"world","keep":keep}),
        )
        .await;
        assert_eq!(translation(&editor, child), expected, "{keep}");
        tool(&mut editor, &context, &client, "undo", json!({})).await;
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn hierarchy_mode_ungroups_a_group_listed_with_its_descendants_once() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let expected = generation(&mut editor, &context, &client).await;
    tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"expected_selection_generation":expected}),
    )
    .await;
    let everything = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(everything);
    editor.sync_fields();
    let mode = tool(
        &mut editor,
        &context,
        &client,
        "set_selection_mode",
        json!({"mode":"hierarchy"}),
    )
    .await;
    assert_eq!(reply(&mode)["outcome"], "changed");
    let expected = generation(&mut editor, &context, &client).await;
    let released = tool(
        &mut editor,
        &context,
        &client,
        "ungroup_selection",
        json!({"expected_selection_generation":expected}),
    )
    .await;
    assert_eq!(reply(&released)["node_ids"].as_array().unwrap().len(), 2);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn refusals_keep_the_scene_selection_and_undo_stack() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let baseline = (editor.history.scene().clone(), editor.selection.clone());
    let depth = editor.history.undo_label();

    // Several objects need the inspected generation; a stale one is refused.
    let none = tool(&mut editor, &context, &client, "group_selection", json!({})).await;
    assert_eq!(code(&none), "multiple_selection");
    let current = generation(&mut editor, &context, &client).await;
    let stale = tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"expected_selection_generation":current.as_u64().unwrap() + 1}),
    )
    .await;
    assert_eq!(code(&stale), "stale_selection");

    // A stale scene token.
    let wrong = call(
        &mut editor,
        &context,
        client.peer(),
        "group_selection",
        json!({"scene_token":uuid::Uuid::new_v4().to_string(),"expected_selection_generation":current}),
    )
    .await;
    assert_eq!(code(&wrong), "stale_scene");

    // A human draft busies the editor; a mesh transform is not a group.
    editor.channels[0] = "2".into();
    let busy = tool(
        &mut editor,
        &context,
        &client,
        "group_selection",
        json!({"expected_selection_generation":current}),
    )
    .await;
    assert_eq!(code(&busy), "busy_editing");
    editor.sync_fields();
    let not_a_group = tool(
        &mut editor,
        &context,
        &client,
        "ungroup_selection",
        json!({"expected_selection_generation":current}),
    )
    .await;
    assert_eq!(code(&not_a_group), "invalid_reparent");

    // Wrong-typed options are protocol errors, never reaching the editor.
    for (name, arguments) in [
        ("group_selection", json!({"under":"sideways"})),
        ("group_selection", json!({"preserve_position":"yes"})),
        ("group_selection", json!({"pivot":7})),
        ("ungroup_selection", json!({"keep":"both"})),
        ("ungroup_selection", json!({"under":null,"extra":1})),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(editor.scene_token.to_string());
        let Err(rmcp::ServiceError::McpError(error)) =
            call_response(&mut editor, &context, client.peer(), name, arguments).await
        else {
            panic!("{name}: expected a protocol error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    assert_eq!(
        (editor.history.scene().clone(), editor.selection.clone()),
        baseline
    );
    assert_eq!(editor.history.undo_label(), depth);

    // Modal dialog and an empty selection.
    editor.deselect_all();
    let empty = tool(&mut editor, &context, &client, "group_selection", json!({})).await;
    assert_eq!(code(&empty), "no_selection");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_ascii_scene_is_read_only_for_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("scene.ma");
    std::fs::write(&source, ascii_fixture::specimen()).unwrap();
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":source}),
    )
    .await;
    assert_eq!(reply(&opened)["outcome"], "document_applied");
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let scene = editor.history.scene().clone();
    for name in ["group_selection", "ungroup_selection"] {
        let refused = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&refused), "read_only_format", "{name}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn a_modal_dialog_refuses_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 1);
    editor.new_document().unwrap();
    let token = editor.scene_token.to_string();
    for name in ["group_selection", "ungroup_selection"] {
        let busy = call(
            &mut editor,
            &context,
            client.peer(),
            name,
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(code(&busy), "busy_editing", "{name}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn ungroup_keep_local_past_f32_and_selected_joints_are_typed_refusals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let child = cubes(&mut editor, 1);
    let made = tool(&mut editor, &context, &client, "group_selection", json!({})).await;
    let group = reply(&made)["node_id"].as_str().unwrap().to_owned();
    tool(
        &mut editor,
        &context,
        &client,
        "edit_node",
        json!({"node_id":group,"scale":[0.001,1,1]}),
    )
    .await;
    // World 1e38 under the group's 1e-3 scale; keep local would leave it at 1e41.
    let moved = tool(
        &mut editor,
        &context,
        &client,
        "edit_node",
        json!({"node_id":child,"translation":[1e41,0,0]}),
    )
    .await;
    assert_ne!(moved.is_error, Some(true), "{moved:?}");
    let scene = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "ungroup_selection",
        json!({"keep":"local"}),
    )
    .await;
    assert_eq!(code(&refused), "world_out_of_range");
    assert_eq!(editor.history.scene(), &scene);

    tool(
        &mut editor,
        &context,
        &client,
        "create_joint",
        json!({"position":[0,0,0]}),
    )
    .await;
    let scene = editor.history.scene().clone();
    for name in ["group_selection", "ungroup_selection"] {
        let refused = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&refused), "invalid_reparent", "{name}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
