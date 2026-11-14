use super::*;
use crate::editor_operations::{ActiveTool, ManipulationSpace};
use azteccraft::native_scene::MoveSpace;

async fn spaces(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> [Value; 3] {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let manipulation = &inspected["inspection"]["manipulation"];
    [
        manipulation["move_space"].clone(),
        manipulation["rotate_space"].clone(),
        manipulation["scale_space"].clone(),
    ]
}

#[tokio::test]
async fn sdk_tool_space_reads_defaults_sets_per_tool_and_shares_the_native_operation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let dirty = editor.is_dirty();
    let tool = editor.active_tool;
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["world", "object", "world"]
    );
    for (name, space, outcome, expected) in [
        ("scale", "object", "changed", ["world", "object", "object"]),
        (
            "scale",
            "object",
            "unchanged",
            ["world", "object", "object"],
        ),
        ("rotate", "world", "changed", ["world", "world", "object"]),
        ("scale", "world", "changed", ["world", "world", "world"]),
        ("move", "parent", "changed", ["parent", "world", "world"]),
        ("move", "parent", "unchanged", ["parent", "world", "world"]),
        ("move", "object", "changed", ["object", "world", "world"]),
        ("scale", "parent", "changed", ["object", "world", "parent"]),
        (
            "scale",
            "parent",
            "unchanged",
            ["object", "world", "parent"],
        ),
        ("scale", "world", "changed", ["object", "world", "world"]),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_space",
            json!({"scene_token":token,"tool":name,"space":space}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(spaces(&mut editor, &context, &client).await, expected);
        assert_eq!(editor.active_tool, tool);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.is_dirty(), dirty);
        assert_eq!(editor.scene_token.to_string(), token);
    }
    // The native selector dispatches the same typed operations.
    editor.native_operation(Operation::SetMoveSpace {
        space: MoveSpace::World,
    });
    editor.native_operation(Operation::SetToolSpace {
        tool: ActiveTool::Rotate,
        space: ManipulationSpace::Object,
    });
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["world", "object", "world"]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_tool_space_rejects_invalid_values_and_honours_every_guard() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token":token,"tool":"move","space":"gimbal"}),
        json!({"scene_token":token,"tool":"rotate","space":"parent"}),
        json!({"scene_token":token,"tool":"select","space":"world"}),
        json!({"scene_token":token,"tool":"rotate","space":"gimbal"}),
        json!({"scene_token":token,"tool":"rotate"}),
        json!({"scene_token":token,"space":"world"}),
        json!({"scene_token":token,"tool":"rotate","space":"world","extra":1}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_tool_space",
            arguments,
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
    }
    // The core operation also refuses a tool without an Axis Orientation, with a typed error.
    assert!(matches!(
        editor.apply_operation(Operation::SetToolSpace {
            tool: ActiveTool::Move,
            space: ManipulationSpace::Object,
        }),
        Err(crate::editor_operations::OperationError::InvalidInput(_))
    ));
    // Rotate has no Parent orientation: the core refuses it with a typed error and no change.
    assert!(matches!(
        editor.apply_operation(Operation::SetToolSpace {
            tool: ActiveTool::Rotate,
            space: ManipulationSpace::Parent,
        }),
        Err(crate::editor_operations::OperationError::InvalidInput(_))
    ));
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["world", "object", "world"]
    );

    let set = |token: &str| json!({"scene_token":token,"tool":"scale","space":"object"});
    let code = |result: rmcp::model::CallToolResult| {
        result.structured_content.unwrap()["error"]["code"].clone()
    };
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(code(stale), "stale_scene");

    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"uniform","space":"world"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(busy), "busy_manipulation");
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    editor.sync_fields();

    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_tool_space",
        set(&token),
    )
    .await;
    assert_eq!(code(modal), "busy_editing");
    assert_eq!(
        spaces(&mut editor, &context, &client).await,
        ["world", "object", "world"]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_scale_parent_begins_remotely_and_natively() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"x","space":"parent"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let update = call(
        &mut editor,
        &context,
        client.peer(),
        "update_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"factor":2.0}),
    )
    .await;
    assert!(update.is_error != Some(true), "{update:?}");
    let finish = call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    assert!(finish.is_error != Some(true), "{finish:?}");
    // A cube at the root: Parent equals World, so X doubles.
    let inspected = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(
        inspected["inspection"]["nodes"][0]["authoring_transform"]["scale"],
        json!([2.0, 1.0, 1.0])
    );
    // The native begin accepts it too: Parent is a state both callers share.
    editor.native_operation(Operation::SetToolSpace {
        tool: ActiveTool::Scale,
        space: ManipulationSpace::Parent,
    });
    let target = editor.selected_move_target().unwrap();
    editor
        .apply_operation(Operation::BeginScale {
            target,
            handle: azteccraft::native_scene::ScaleHandle::X,
            space: ManipulationSpace::Parent,
            expected_selection_generation: None,
        })
        .unwrap();
    assert!(editor.history.active_manipulation().is_some());
    editor.history.cancel_active_move().unwrap();
    assert!(matches!(
        editor.apply_operation(Operation::BeginRotate {
            target: editor.selected_move_target().unwrap(),
            axis: azteccraft::native_scene::RotateAxis::X,
            space: ManipulationSpace::Parent,
        }),
        Err(crate::editor_operations::OperationError::InvalidInput(_))
    ));
    assert!(editor.history.active_manipulation().is_none());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn child_scale(editor: &Editor) -> [f64; 3] {
    let scene = editor.history.scene();
    scene
        .nodes()
        .iter()
        .find(|node| node.name() == "c")
        .unwrap()
        .transform()
        .scale
}

#[tokio::test]
async fn sdk_scale_parent_under_a_rotated_parent_matches_native_and_undoes_and_a_skewed_parent_draws_no_handles()
 {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.history = crate::app::parent_fixture_tests::parent_child_editor(
        None,
        std::f64::consts::FRAC_PI_2,
        0.0,
    )
    .history;
    crate::app::parent_fixture_tests::select_child(&mut editor);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let target = editor.selected_move_target().unwrap();
    let node = target.node.to_decimal();
    let path: Vec<_> = target.path.iter().map(|id| id.to_decimal()).collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":node,"instance_path":path,"handle":"x","space":"parent"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    for (tool, args) in [
        ("update_manipulation", json!({"factor":2.0})),
        ("finish_manipulation", json!({"finish":"commit"})),
    ] {
        let mut args = args;
        args["scene_token"] = json!(token);
        args["manipulation_id"] = json!(id);
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert!(result.is_error != Some(true), "{result:?}");
    }
    let remote = child_scale(&editor);
    assert_eq!(remote, [2.0, 1.0, 1.0]);
    // Undo reverts the remote edit.
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(child_scale(&editor), [1.0; 3]);
    // The native operation on the same scene gives the same result.
    editor.native_operation(Operation::SetToolSpace {
        tool: ActiveTool::Scale,
        space: ManipulationSpace::Parent,
    });
    let reply = editor
        .apply_operation(Operation::BeginScale {
            target,
            handle: azteccraft::native_scene::ScaleHandle::X,
            space: ManipulationSpace::Parent,
            expected_selection_generation: None,
        })
        .unwrap();
    let crate::editor_operations::OperationReply::ManipulationStarted { manipulation_id } = reply
    else {
        panic!("{reply:?}");
    };
    let id = azteccraft::history::ManipulationId::from_decimal(&manipulation_id).unwrap();
    editor
        .apply_operation(Operation::UpdateManipulation {
            id,
            value: crate::editor_operations::ManipulationValue::Factor(2.0),
        })
        .unwrap();
    editor
        .apply_operation(Operation::FinishMove {
            id,
            finish: azteccraft::history::ManipulationFinish::Commit,
        })
        .unwrap();
    assert_eq!(child_scale(&editor), remote);
    assert!(editor.history.undo());
    assert_eq!(child_scale(&editor), [1.0; 3]);
    // A skewed Parent frame (grandparent scale under a rotated parent) draws no native handles
    // (SkewedFrame). The core scales local channels and reads no parents' map, so the remote
    // begin starts; the native begin gives the same verdict.
    editor.history = crate::app::parent_fixture_tests::parent_child_editor(
        Some([2.0, 1.0, 1.0]),
        std::f64::consts::FRAC_PI_4,
        0.0,
    )
    .history;
    crate::app::parent_fixture_tests::select_child(&mut editor);
    frame(&mut editor, &context, vec![]);
    editor.native_operation(Operation::SetToolSpace {
        tool: ActiveTool::Scale,
        space: ManipulationSpace::Parent,
    });
    let refusal = editor.manipulation_frame(ActiveTool::Scale).err().unwrap();
    assert!(refusal.to_string().contains("skew"), "{refusal}");
    let token = editor.scene_token.to_string();
    let target = editor.selected_move_target().unwrap();
    let before = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"kind":"scale","node_id":target.node.to_decimal(),
            "instance_path":target.path.iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
            "handle":"x","space":"parent"}),
    )
    .await;
    let started = refused.structured_content.unwrap();
    assert_eq!(started["outcome"], "manipulation_started");
    let id = started["manipulation_id"].as_str().unwrap().to_owned();
    let cancel = call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    assert!(cancel.is_error != Some(true), "{cancel:?}");
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
