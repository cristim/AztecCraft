use super::*;

async fn code(result: rmcp::model::CallToolResult) -> String {
    result.structured_content.unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

async fn snapped(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["snap_to_grid"]
        .clone()
}

#[tokio::test]
async fn sdk_snap_mode_snaps_a_move_and_guards_the_session_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<String> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let x = |editor: &Editor| editor.history.scene().nodes()[0].transform().translation[0];
    assert_eq!(snapped(&mut editor, &context, &client).await, false);

    for (grid, outcome) in [
        (true, "changed"),
        (true, "unchanged"),
        (false, "changed"),
        (true, "changed"),
    ] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_snap_mode",
            json!({"scene_token": token, "grid": grid}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(snapped(&mut editor, &context, &client).await, grid);
    }

    // Move to 0.3 unsnapped, then 2.4 more with snap on lands the pivot on 3.
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_snap_mode",
        json!({"scene_token": token, "grid": false}),
    )
    .await;
    for (distance, expected) in [(0.3, 0.3), (2.4, 3.0)] {
        if expected == 3.0 {
            call(
                &mut editor,
                &context,
                client.peer(),
                "set_snap_mode",
                json!({"scene_token": token, "grid": true}),
            )
            .await;
        }
        let begin = call(
            &mut editor,
            &context,
            client.peer(),
            "begin_manipulation",
            json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
        )
        .await
        .structured_content
        .unwrap();
        let id = begin["manipulation_id"].as_str().unwrap().to_owned();
        call(
            &mut editor,
            &context,
            client.peer(),
            "update_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"distance":distance}),
        )
        .await;
        assert_eq!(x(&editor), expected);
        if expected == 3.0 {
            let busy = call(
                &mut editor,
                &context,
                client.peer(),
                "set_snap_mode",
                json!({"scene_token": token, "grid": false}),
            )
            .await;
            assert_eq!(code(busy).await, "busy_manipulation");
            assert_eq!(snapped(&mut editor, &context, &client).await, true);
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
            let value = inspected["inspection"]["manipulation"]["active"]["value"]
                .as_f64()
                .unwrap();
            assert!((value - 2.7).abs() < 1e-12, "{value}");
        }
        call(
            &mut editor,
            &context,
            client.peer(),
            "finish_manipulation",
            json!({"scene_token":token,"manipulation_id":id,"finish":"commit"}),
        )
        .await;
    }
    assert_eq!(x(&editor), 3.0);

    // An Object axis between grid axes is refused at begin, with nothing started or changed.
    for (name, arguments) in [
        (
            "set_tool_space",
            json!({"scene_token":token,"tool":"move","space":"object"}),
        ),
        (
            "edit_node",
            json!({"scene_token":token,"node_id":node,"rotation_degrees":[null,30,null]}),
        ),
    ] {
        call(&mut editor, &context, client.peer(), name, arguments).await;
    }
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"}),
    )
    .await;
    assert_eq!(code(refused).await, "invalid_input");
    assert!(editor.history.active_manipulation().is_none());
    assert_eq!(editor.history.scene(), &scene);

    // Malformed arguments and a stale token change nothing.
    for arguments in [
        json!({"scene_token": token}),
        json!({"grid": true}),
        json!({"scene_token": token, "grid": null}),
        json!({"scene_token": token, "grid": "true"}),
        json!({"scene_token": token, "grid": 1}),
        json!({"scene_token": token, "grid": true, "curve": true}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "set_snap_mode",
                arguments
            )
            .await
            .is_err()
        );
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_snap_mode",
        json!({"scene_token": Uuid::new_v4().to_string(), "grid": false}),
    )
    .await;
    assert_eq!(code(stale).await, "stale_scene");
    assert_eq!(snapped(&mut editor, &context, &client).await, true);
    assert_eq!(editor.history.scene(), &scene);

    // A native setter shares the state the tool reads.
    editor.native_operation(Operation::SetSnapToGrid { enabled: false });
    assert_eq!(snapped(&mut editor, &context, &client).await, false);

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
