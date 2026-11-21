//! Issue #313 slice 1 over the official client: the guards, input checks and typed refusals both
//! transformation tools share with the other mutations.
use super::pivot_edit::{code, cubes, inspected, reply, tool};
use super::transform_ops::{TOOLS, pose};
use super::*;
use uuid::Uuid;

#[tokio::test]
async fn selection_token_and_input_refusals_leave_the_scene_untouched() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    cubes(&mut editor, 2);
    let generation = inspected(&mut editor, &context, &client).await["selection_generation"]
        .as_u64()
        .unwrap();
    let scene = editor.history.scene().clone();
    let revision = editor.history.undo_label();
    for name in TOOLS {
        let missing = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&missing), "multiple_selection", "{name}");
        let stale = tool(
            &mut editor,
            &context,
            &client,
            name,
            json!({"expected_selection_generation":generation + 1}),
        )
        .await;
        assert_eq!(code(&stale), "stale_selection", "{name}");
        let stale_scene = call(
            &mut editor,
            &context,
            client.peer(),
            name,
            json!({"scene_token":Uuid::new_v4().to_string(),"expected_selection_generation":generation}),
        )
        .await;
        assert_eq!(code(&stale_scene), "stale_scene", "{name}");
        let none = tool(
            &mut editor,
            &context,
            &client,
            name,
            json!({"translate":false,"rotate":false,"scale":false,"expected_selection_generation":generation}),
        )
        .await;
        assert_eq!(code(&none), "invalid_transform_edit", "{name}");
        for bad in [json!({"translate":"yes"}), json!({"unknown":true})] {
            let mut args = bad.clone();
            args["scene_token"] = json!(editor.scene_token.to_string());
            let Err(rmcp::ServiceError::McpError(error)) =
                call_response(&mut editor, &context, client.peer(), name, args).await
            else {
                panic!("{name} {bad}: expected a protocol error")
            };
            assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        }
        editor.selection_mode = SelectionMode::Face;
        let component = tool(
            &mut editor,
            &context,
            &client,
            name,
            json!({"expected_selection_generation":generation}),
        )
        .await;
        assert_eq!(code(&component), "invalid_input", "{name}");
        editor.selection_mode = SelectionMode::Object;
    }
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    for name in TOOLS {
        let empty = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&empty), "no_selection", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    assert_eq!(editor.history.undo_label(), revision);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn draft_manipulation_modal_and_closing_states_refuse_both_tools() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    pose(&mut editor, &context, &client, &node).await;
    let scene = editor.history.scene().clone();

    editor.channels[0] = "5".into();
    editor.channel_intents[0] = true;
    for name in TOOLS {
        let busy = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&busy), "busy_editing", "{name}");
    }
    editor.channel_intents[0] = false;
    editor.sync_fields();

    let path: Vec<String> = editor.history.scene().transform_paths().unwrap()[0]
        .path
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begun = tool(
        &mut editor,
        &context,
        &client,
        "begin_manipulation",
        json!({"kind":"move","node_id":node,"instance_path":path,"axis":"x"}),
    )
    .await;
    let manipulation = reply(&begun)["manipulation_id"]
        .as_str()
        .unwrap()
        .to_owned();
    for name in TOOLS {
        let busy = tool(&mut editor, &context, &client, name, json!({})).await;
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
    for name in TOOLS {
        let modal = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&modal), "busy_editing", "{name}");
    }
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn keyed_mirrored_and_read_only_refusals_are_typed_and_change_nothing() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let node = cubes(&mut editor, 1);
    let mirrored = tool(
        &mut editor,
        &context,
        &client,
        "set_transform",
        json!({"node_id":node,"translation":[0,0,0],"rotation_degrees":[0,0,0],"scale":[-1,1,1]}),
    )
    .await;
    reply(&mirrored);
    let scene = editor.history.scene().clone();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "freeze_transformations",
        json!({}),
    )
    .await;
    assert_eq!(code(&refused), "invalid_transform_edit");
    assert_eq!(editor.history.scene(), &scene);
    let reset = tool(
        &mut editor,
        &context,
        &client,
        "reset_transformations",
        json!({}),
    )
    .await;
    assert_eq!(
        reply(&reset)["outcome"],
        "changed",
        "reset has no mirror rule"
    );

    let keyed = tool(
        &mut editor,
        &context,
        &client,
        "set_keyframe",
        json!({"node_ids":[node]}),
    )
    .await;
    assert_eq!(reply(&keyed)["outcome"], "changed");
    let scene = editor.history.scene().clone();
    for name in TOOLS {
        let refused = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&refused), "invalid_transform_edit", "{name}");
        assert_eq!(editor.history.scene(), &scene);
    }

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn an_opened_maya_ascii_scene_is_read_only_for_both_tools() {
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
    let path = editor.history.scene().transform_paths().unwrap()[0].clone();
    editor.set_selection_members_for_test(vec![path]);
    editor.sync_fields();
    let scene = editor.history.scene().clone();
    for name in TOOLS {
        let refused = tool(&mut editor, &context, &client, name, json!({})).await;
        assert_eq!(code(&refused), "read_only_format", "{name}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn freezing_an_instanced_transform_or_below_it_is_a_typed_transform_refusal() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut scene = azteccraft::native_scene::NativeScene::default();
    let [x1, x2, a, b] =
        ["x1", "x2", "a", "b"].map(|name| scene.create(Primitive::Cube, name).unwrap());
    scene.set_parents_for_test(a, &[x1, x2]).unwrap();
    scene.set_parents_for_test(b, &[a]).unwrap();
    editor.history = azteccraft::history::History::new(scene);
    for node in [a, b, x1] {
        let path = editor
            .history
            .scene()
            .transform_paths()
            .unwrap()
            .into_iter()
            .find(|path| path.node == node)
            .unwrap();
        editor.set_selection_members_for_test(vec![path]);
        editor.sync_fields();
        let scene = editor.history.scene().clone();
        let refused = tool(
            &mut editor,
            &context,
            &client,
            "freeze_transformations",
            json!({}),
        )
        .await;
        assert_eq!(code(&refused), "invalid_transform_edit", "{node:?}");
        let message = refused.structured_content.as_ref().unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(message.contains("frozen"), "{message}");
        assert!(!message.contains("reparent"), "{message}");
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
