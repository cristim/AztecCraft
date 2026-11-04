use super::*;

async fn three_selected(
    context: &egui::Context,
) -> (
    Editor,
    rmcp::service::RunningService<RoleClient, ()>,
    tokio::task::JoinHandle<()>,
    Vec<azteccraft::native_scene::TransformTarget>,
) {
    let (mut editor, client, server) = start(context).await;
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let members = editor.history.scene().transform_paths().unwrap();
    editor.set_selection_members_for_test(members.clone());
    frame(&mut editor, context, vec![]);
    (editor, client, server, members)
}

fn begin_args(
    editor: &Editor,
    target: &azteccraft::native_scene::TransformTarget,
    generation: Option<Value>,
) -> Value {
    let mut args = json!({
        "scene_token": editor.scene_token.to_string(),
        "kind": "move",
        "node_id": target.node.to_decimal(),
        "instance_path": target.path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
        "axis": "x",
    });
    if let Some(generation) = generation {
        args["expected_selection_generation"] = generation;
    }
    args
}

async fn generation(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> u64 {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["selection_generation"]
        .as_u64()
        .unwrap()
}

fn code(result: CallToolResult) -> String {
    result.structured_content.unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn sdk_move_with_the_current_generation_moves_every_selected_object_in_one_undo_step() {
    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members.last().unwrap().clone();
    let before = editor.history.scene().clone();
    let current = generation(&mut editor, &context, &client).await;
    let args = begin_args(&editor, &lead, Some(json!(current)));
    let begun = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await;
    let begun = begun.structured_content.unwrap();
    assert_eq!(begun["outcome"], "manipulation_started");
    let id = begun["manipulation_id"].as_str().unwrap().to_owned();
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"distance":2.0}),
    )
    .await;
    let active = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap()["inspection"]["manipulation"]["active"]
        .clone();
    assert_eq!(
        (
            active["member_count"].clone(),
            active["covered_count"].clone()
        ),
        (json!(3), json!(0))
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    for member in &members {
        let x = editor
            .history
            .scene()
            .node(member.node)
            .unwrap()
            .transform()
            .translation[0];
        assert_eq!(x, 2.0);
    }
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_move_refuses_a_stale_generation_a_missing_one_and_a_non_lead_target() {
    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members[2].clone();
    let stale = generation(&mut editor, &context, &client).await;
    // A native toggle of a middle member keeps the lead and changes the generation.
    editor.native_operation(Operation::ModifySelection {
        change: crate::editor_operations::SelectionChange::Toggle,
        node: members[1].node,
        instance_path: Some(members[1].path.clone()),
    });
    let current = generation(&mut editor, &context, &client).await;
    assert_ne!(stale, current);
    let scene = editor.history.scene().clone();
    for (target, argument, expected) in [
        (&lead, Some(json!(stale)), "invalid_input"),
        (&lead, None, "multiple_selection"),
        (&members[0], Some(json!(current)), "multiple_selection"),
    ] {
        let args = begin_args(&editor, target, argument.clone());
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            args,
        )
        .await;
        assert_eq!(code(result), expected, "{argument:?}");
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.active_manipulation().is_none());
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_move_of_one_selected_object_checks_the_generation_when_given() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let lead = editor.selection.lead().unwrap().clone();
    let current = generation(&mut editor, &context, &client).await;
    for generation in [json!(current + 1), json!(current)] {
        let args = begin_args(&editor, &lead, Some(generation.clone()));
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            args,
        )
        .await;
        if generation == json!(current) {
            assert_eq!(
                result.structured_content.unwrap()["outcome"],
                "manipulation_started"
            );
            editor.cancel_move().unwrap();
        } else {
            assert_eq!(code(result), "invalid_input");
            assert!(editor.history.active_manipulation().is_none());
        }
    }
    let args = begin_args(&editor, &lead, None);
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["outcome"],
        "manipulation_started"
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_expected_selection_generation_of_the_wrong_type_is_invalid_params() {
    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members[2].clone();
    let scene = editor.history.scene().clone();
    for bad in [
        json!("1"),
        json!(-1),
        json!(1.5),
        json!(18446744073709551616.0),
        json!([1]),
        json!(true),
    ] {
        let args = begin_args(&editor, &lead, Some(bad.clone()));
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            args,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {bad}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS, "{bad}");
        assert_eq!(editor.history.scene(), &scene);
        assert!(editor.history.active_manipulation().is_none());
    }
    // The argument belongs to Move only.
    let mut rotate = begin_args(&editor, &lead, Some(json!(0)));
    rotate["kind"] = json!("rotate");
    rotate["space"] = json!("world");
    let rejected = call_response(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        rotate,
    )
    .await;
    let Err(rmcp::ServiceError::McpError(error)) = rejected else {
        panic!("expected a JSON-RPC error");
    };
    assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
