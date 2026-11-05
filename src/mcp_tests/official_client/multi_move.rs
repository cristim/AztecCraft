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
    // Restore the middle member, then remove the old lead: the stale request
    // still names that old lead while two objects remain selected.
    for member in [&members[1], &lead] {
        editor.native_operation(Operation::ModifySelection {
            change: crate::editor_operations::SelectionChange::Toggle,
            node: member.node,
            instance_path: Some(member.path.clone()),
        });
    }
    let args = begin_args(&editor, &lead, Some(json!(current)));
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await;
    assert_eq!(code(result), "invalid_input");
    assert_eq!(editor.history.scene(), &scene);
    assert!(editor.history.active_manipulation().is_none());
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

#[tokio::test]
async fn sdk_multi_move_equals_the_native_result_for_the_same_selection_and_delta() {
    use crate::editor_operations::MoveDistance;
    use azteccraft::history::{ManipulationFinish, ManipulationId};
    let mut native = Editor::default();
    for _ in 0..3 {
        native.create(Primitive::Cube);
    }
    let members = native.history.scene().transform_paths().unwrap();
    native.set_selection_members_for_test(members.clone());
    let lead = members.last().unwrap().clone();
    let reply = native
        .apply_operation(Operation::BeginMove {
            target: lead,
            axis: azteccraft::native_scene::WorldAxis::X,
            expected_selection_generation: None,
        })
        .unwrap();
    let crate::editor_operations::OperationReply::ManipulationStarted { manipulation_id } = reply
    else {
        panic!("expected a started manipulation");
    };
    let id = ManipulationId::from_decimal(&manipulation_id).unwrap();
    native
        .apply_operation(Operation::UpdateMove {
            id,
            distance: MoveDistance::Centimeters(2.0),
        })
        .unwrap();
    native
        .apply_operation(Operation::FinishMove {
            id,
            finish: ManipulationFinish::Commit,
        })
        .unwrap();

    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members.last().unwrap().clone();
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
    let id = begun["manipulation_id"].as_str().unwrap().to_owned();
    let token = editor.scene_token.to_string();
    for (tool, args) in [
        (
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"distance":2.0}),
        ),
        (
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
        ),
    ] {
        call(&mut editor, &context, client.peer(), tool, args).await;
    }
    for member in &members {
        let moved = |scene: &azteccraft::native_scene::NativeScene| {
            scene
                .node(member.node)
                .unwrap()
                .transform()
                .values()
                .map(f64::to_bits)
        };
        assert_eq!(moved(editor.history.scene()), moved(native.history.scene()));
    }
    assert_eq!(editor.history.undo_label(), native.history.undo_label());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_selection_change_during_an_active_multi_move_is_busy_manipulation() {
    let context = egui::Context::default();
    let (mut editor, client, server, members) = three_selected(&context).await;
    let lead = members.last().unwrap().clone();
    let current = generation(&mut editor, &context, &client).await;
    let args = begin_args(&editor, &lead, Some(json!(current)));
    call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await;
    let token = editor.scene_token.to_string();
    let before = editor.selection.members().to_vec();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "modify_selection",
        json!({"scene_token":token,"change":"toggle","node_id":members[0].node.to_decimal(),
               "instance_path":members[0].path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>()}),
    )
    .await;
    assert_eq!(code(result), "busy_manipulation");
    assert_eq!(editor.selection.members(), before);
    assert_eq!(
        editor.history.active_manipulation().unwrap().member_count(),
        3
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_multi_move_refused_member_has_its_cause_code_and_names_the_member() {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene_with_units};
    let object = |name: &[u8], shape: &[u8]| {
        [
            group(b"XFRM", &creation(name, None), 4),
            group(
                b"DMSH",
                &[
                    creation(shape, Some(name)),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat()
    };
    let original = scene_with_units(
        &[
            object(b"alpha", b"alpha_shape"),
            object(b"bravo", b"bravo_shape"),
        ]
        .concat(),
        b"cm",
        b"deg",
    );
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("two-bare-objects.mb");
    std::fs::write(&source, &original).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let old_token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":old_token,"path":source}),
    )
    .await;
    let members = editor.history.scene().transform_paths().unwrap();
    assert_eq!(members.len(), 2);
    let token = editor.scene_token.to_string();
    for (tool, member, change) in [
        ("select_node", &members[0], None),
        ("modify_selection", &members[1], Some("add")),
    ] {
        let mut args = json!({"scene_token":token,"node_id":member.node.to_decimal(),
            "instance_path":member.path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>()});
        if let Some(change) = change {
            args["change"] = json!(change);
        }
        call(&mut editor, &context, client.peer(), tool, args).await;
    }
    assert_eq!(editor.selection.len(), 2);
    let current = generation(&mut editor, &context, &client).await;
    let lead = members.last().unwrap().clone();
    let first = editor
        .history
        .scene()
        .node(members[0].node)
        .unwrap()
        .name()
        .to_string();
    let before = editor.history.scene().clone();
    let args = begin_args(&editor, &lead, Some(json!(current)));
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        args,
    )
    .await;
    let error = &refused.structured_content.unwrap()["error"];
    assert_eq!(error["code"], "absent_channel_leaf");
    assert!(
        error["message"].as_str().unwrap().contains(&first),
        "{error}"
    );
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
