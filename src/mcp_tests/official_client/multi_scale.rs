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
        "kind": "scale",
        "node_id": target.node.to_decimal(),
        "instance_path": target.path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
        "handle": "uniform",
        "space": "object",
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
async fn sdk_scale_with_the_current_generation_scales_every_selected_object_in_one_undo_step() {
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
    .await
    .structured_content
    .unwrap();
    assert_eq!(begun["outcome"], "manipulation_started");
    let id = begun["manipulation_id"].as_str().unwrap().to_owned();
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"factor":2.0}),
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
            active["kind"].clone(),
            active["member_count"].clone(),
            active["covered_count"].clone()
        ),
        (json!("scale"), json!(3), json!(0))
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
        let scale = editor
            .history
            .scene()
            .node(member.node)
            .unwrap()
            .transform()
            .scale;
        assert_eq!(scale, [2.0; 3]);
    }
    assert_eq!(editor.history.undo_label().as_deref(), Some("Scale"));
    assert!(editor.history.undo());
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_scale_refuses_a_stale_generation_a_missing_one_and_a_non_lead_target() {
    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members[2].clone();
    let stale = generation(&mut editor, &context, &client).await;
    editor.native_operation(Operation::ModifySelection {
        change: crate::editor_operations::SelectionChange::Toggle,
        node: members[1].node,
        instance_path: Some(members[1].path.clone()),
    });
    let current = generation(&mut editor, &context, &client).await;
    assert_ne!(stale, current);
    let scene = editor.history.scene().clone();
    for (target, argument, expected) in [
        (&lead, Some(json!(stale)), "stale_selection"),
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
async fn sdk_multi_scale_equals_the_native_result_for_the_same_selection_and_factor() {
    let context = egui::Context::default();
    let (mut remote_editor, client, server, members) = three_selected(&context).await;
    let lead = members[2].clone();
    let current = generation(&mut remote_editor, &context, &client).await;
    let args = begin_args(&remote_editor, &lead, Some(json!(current)));
    let begun = call(
        &mut remote_editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await
    .structured_content
    .unwrap();
    let id = begun["manipulation_id"].as_str().unwrap().to_owned();
    let token = remote_editor.scene_token.to_string();
    call(
        &mut remote_editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"factor":1.5}),
    )
    .await;
    call(
        &mut remote_editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    let (mut native, _client2, _server2, native_members) = three_selected(&context).await;
    let begin = Operation::BeginScale {
        target: native_members[2].clone(),
        handle: azteccraft::native_scene::ScaleHandle::Uniform,
        space: crate::editor_operations::ManipulationSpace::Object,
        expected_selection_generation: None,
    };
    native.native_operation(begin);
    let native_id = native.history.active_manipulation().unwrap().id;
    native.native_operation(Operation::UpdateManipulation {
        id: native_id,
        value: crate::editor_operations::ManipulationValue::Factor(1.5),
    });
    native.native_operation(Operation::FinishMove {
        id: native_id,
        finish: azteccraft::history::ManipulationFinish::Commit,
    });
    let scales = |editor: &Editor, members: &[azteccraft::native_scene::TransformTarget]| {
        members
            .iter()
            .map(|m| {
                editor
                    .history
                    .scene()
                    .node(m.node)
                    .unwrap()
                    .transform()
                    .scale
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        scales(&remote_editor, &members),
        scales(&native, &native_members)
    );
    assert_eq!(scales(&native, &native_members), vec![[1.5; 3]; 3]);
    client.cancel().await.unwrap();
    drop(remote_editor);
    server.await.unwrap();
}
