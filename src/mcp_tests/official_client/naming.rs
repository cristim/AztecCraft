//! Issue #313 slice 3 over the official client: `prefix_hierarchy_names` and
//! `search_replace_names` through the real Editor protocol, their guards and typed refusals.
use super::pivot_edit::{code, inspected, reply, tool};
use super::*;
use azteccraft::native_scene::ReparentKeep;
use uuid::Uuid;

const PREFIX: &str = "prefix_hierarchy_names";
const REPLACE: &str = "search_replace_names";

fn names(editor: &Editor) -> Vec<String> {
    editor
        .history
        .scene()
        .nodes()
        .iter()
        .map(|n| n.name().to_owned())
        .collect()
}

/// pCube1 > pCube2 > pCube3 plus a lone pCube4; only the root pCube1 is selected.
fn nested(editor: &mut Editor) {
    for _ in 0..4 {
        editor.create(Primitive::Cube);
    }
    let ids: Vec<_> = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .iter()
        .map(|t| t.node)
        .collect();
    for (child, parent) in [(1, 0), (2, 1)] {
        editor
            .apply_operation(Operation::Reparent {
                id: ids[child],
                parent: Some(ids[parent]),
                keep: ReparentKeep::Local,
            })
            .unwrap();
    }
    let root = editor
        .history
        .scene()
        .transform_paths()
        .unwrap()
        .into_iter()
        .find(|t| t.node == ids[0])
        .unwrap();
    editor.set_selection_members_for_test(vec![root]);
    editor.sync_fields();
}

#[tokio::test]
async fn prefix_reaches_the_hierarchy_in_one_named_undo_step_with_an_exact_restore() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    nested(&mut editor);
    let before = editor.history.scene().clone();
    let done = tool(
        &mut editor,
        &context,
        &client,
        PREFIX,
        json!({"prefix":"rig_"}),
    )
    .await;
    assert_eq!(reply(&done)["outcome"], "changed");
    assert_eq!(
        names(&editor),
        ["rig_pCube1", "rig_pCube2", "rig_pCube3", "pCube4"]
    );
    let seen = inspected(&mut editor, &context, &client).await;
    assert_eq!(seen["undo_label"], "Prefix Hierarchy Names");
    let after = editor.history.scene().clone();
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert_eq!(editor.history.scene(), &before);
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert_eq!(editor.history.scene(), &after);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn search_replace_scopes_case_and_undo_label() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    nested(&mut editor);
    let before = editor.history.scene().clone();
    let selected = tool(
        &mut editor,
        &context,
        &client,
        REPLACE,
        json!({"search":"Cube","replace":"Box"}),
    )
    .await;
    assert_eq!(reply(&selected)["outcome"], "changed");
    assert_eq!(names(&editor), ["pBox1", "pCube2", "pCube3", "pCube4"]);
    assert_eq!(
        inspected(&mut editor, &context, &client).await["undo_label"],
        "Search and Replace Names"
    );
    let insensitive = tool(
        &mut editor,
        &context,
        &client,
        REPLACE,
        json!({"search":"CUBE","replace":"Ring","scope":"hierarchy","match_case":false}),
    )
    .await;
    assert_eq!(reply(&insensitive)["outcome"], "changed");
    assert_eq!(names(&editor), ["pBox1", "pRing2", "pRing3", "pCube4"]);
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let all = tool(
        &mut editor,
        &context,
        &client,
        REPLACE,
        json!({"search":"p","replace":"q","scope":"all"}),
    )
    .await;
    assert_eq!(reply(&all)["outcome"], "changed");
    assert_eq!(names(&editor), ["qBox1", "qRing2", "qRing3", "qCube4"]);
    for _ in 0..3 {
        tool(&mut editor, &context, &client, "undo", json!({})).await;
    }
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn typed_refusals_and_bad_input_leave_the_scene_and_history_untouched() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    nested(&mut editor);
    let scene = editor.history.scene().clone();
    let revision = editor.history.undo_label();
    for (tool_name, args, want) in [
        (PREFIX, json!({"prefix":""}), "invalid_name_edit"),
        (PREFIX, json!({"prefix":"1"}), "invalid_name"),
        (PREFIX, json!({"prefix":"a b"}), "invalid_name"),
        (
            REPLACE,
            json!({"search":"","replace":"x"}),
            "invalid_name_edit",
        ),
        (
            REPLACE,
            json!({"search":"zzz","replace":"x"}),
            "invalid_name_edit",
        ),
        (
            REPLACE,
            json!({"search":"pCube1","replace":"","scope":"hierarchy"}),
            "invalid_name",
        ),
        (
            REPLACE,
            json!({"search":"pCube1","replace":"pCube4"}),
            "name_collision",
        ),
        (
            REPLACE,
            json!({"search":"1","replace":"4","scope":"all"}),
            "name_collision",
        ),
    ] {
        let refused = tool(&mut editor, &context, &client, tool_name, args.clone()).await;
        assert_eq!(code(&refused), want, "{tool_name} {args}");
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        PREFIX,
        json!({"scene_token":Uuid::new_v4().to_string(),"prefix":"a_"}),
    )
    .await;
    assert_eq!(code(&stale), "stale_scene");
    for (tool_name, bad) in [
        (PREFIX, json!({"prefix":1})),
        (PREFIX, json!({})),
        (PREFIX, json!({"prefix":"a","unknown":true})),
        (
            REPLACE,
            json!({"search":"a","replace":"b","scope":"nowhere"}),
        ),
        (REPLACE, json!({"search":"a"})),
        (
            REPLACE,
            json!({"search":"a","replace":"b","match_case":"yes"}),
        ),
        (PREFIX, json!({"prefix":"x".repeat(4097)})),
    ] {
        let mut args = bad.clone();
        args["scene_token"] = json!(editor.scene_token.to_string());
        let Err(rmcp::ServiceError::McpError(error)) =
            call_response(&mut editor, &context, client.peer(), tool_name, args).await
        else {
            panic!("{bad}: expected a protocol error")
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    editor.selection_mode = SelectionMode::Face;
    let component = tool(
        &mut editor,
        &context,
        &client,
        PREFIX,
        json!({"prefix":"a_"}),
    )
    .await;
    assert_eq!(code(&component), "invalid_input");
    editor.selection_mode = SelectionMode::Object;
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), revision);

    let all = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(all);
    editor.sync_fields();
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let missing = tool(
        &mut editor,
        &context,
        &client,
        PREFIX,
        json!({"prefix":"a_"}),
    )
    .await;
    assert_eq!(code(&missing), "multiple_selection");
    let outdated = tool(
        &mut editor,
        &context,
        &client,
        PREFIX,
        json!({"prefix":"a_","expected_selection_generation":generation + 1}),
    )
    .await;
    assert_eq!(code(&outdated), "stale_selection");
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let none = tool(
        &mut editor,
        &context,
        &client,
        PREFIX,
        json!({"prefix":"a_"}),
    )
    .await;
    assert_eq!(code(&none), "no_selection");
    let none = tool(
        &mut editor,
        &context,
        &client,
        REPLACE,
        json!({"search":"p","replace":"q"}),
    )
    .await;
    assert_eq!(code(&none), "no_selection");
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn draft_manipulation_modal_and_closing_states_refuse_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    nested(&mut editor);
    let scene = editor.history.scene().clone();
    let calls = [
        (PREFIX, json!({"prefix":"a_"})),
        (REPLACE, json!({"search":"p","replace":"q","scope":"all"})),
    ];

    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    for (name, args) in &calls {
        let busy = tool(&mut editor, &context, &client, name, args.clone()).await;
        assert_eq!(code(&busy), "busy_editing", "{name}");
    }
    editor.channel_intents[0] = false;
    editor.sync_fields();

    let first = editor.history.scene().transform_paths().unwrap()[0].clone();
    let path: Vec<String> = first.path.iter().map(|id| id.to_decimal()).collect();
    let begun = tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"kind":"move","node_id":first.node.to_decimal(),"instance_path":path,"axis":"x"}),
    )
    .await;
    let manipulation = reply(&begun)["manipulation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (name, args) in &calls {
        let busy = tool(&mut editor, &context, &client, name, args.clone()).await;
        assert_eq!(code(&busy), "busy_manipulation", "{name}");
    }
    tool(
        &mut editor,
        &context,
        &client,
        "finish_manipulation",
        json!({"manipulation_id":manipulation,"finish":"cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &scene);

    editor.request_close();
    for (name, args) in &calls {
        let modal = tool(&mut editor, &context, &client, name, args.clone()).await;
        assert_eq!(code(&modal), "busy_editing", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_ascii_scene_is_read_only() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("specimen.ma");
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
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.into_iter().take(1).collect());
    editor.sync_fields();
    let scene = editor.history.scene().clone();
    for (name, args) in [
        (PREFIX, json!({"prefix":"a_"})),
        (REPLACE, json!({"search":"p","replace":"q","scope":"all"})),
    ] {
        let refused = tool(&mut editor, &context, &client, name, args).await;
        assert_eq!(code(&refused), "read_only_format", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_binary_scene_is_refused_as_unsupported_edit() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("specimen.mb");
    std::fs::write(
        &source,
        appearance_fixture::appearance_specimen_with_users(2),
    )
    .unwrap();
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":source}),
    )
    .await;
    assert_eq!(reply(&opened)["outcome"], "document_applied");
    let paths = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(paths.into_iter().take(1).collect());
    editor.sync_fields();
    let scene = editor.history.scene().clone();
    for (name, args) in [
        (PREFIX, json!({"prefix":"a_"})),
        (REPLACE, json!({"search":"p","replace":"q","scope":"all"})),
    ] {
        let refused = tool(&mut editor, &context, &client, name, args).await;
        assert_eq!(code(&refused), "unsupported_edit", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
