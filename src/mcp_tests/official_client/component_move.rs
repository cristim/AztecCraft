//! A remote component Move over the real Editor protocol: `begin_manipulation` `move` in Vertex
//! selection names the inspected `selection_generation`, `update_manipulation` and
//! `finish_manipulation` drive the same transaction a native drag uses (#41).
use super::*;

struct Setup {
    token: String,
    node: String,
    path: Vec<Value>,
    target: Value,
}

async fn cube_in_vertex_mode(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    vertex: u64,
) -> Setup {
    let token = editor.scene_token.to_string();
    let created = call(
        editor,
        context,
        peer,
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await
    .structured_content
    .unwrap();
    let node = created["node_id"].as_str().unwrap().to_owned();
    let inspected = call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let found = inspected["inspection"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == node)
        .unwrap()
        .clone();
    let path: Vec<Value> = found["instance_paths"][0].as_array().unwrap().clone();
    let mut full = path.clone();
    full.push(found["shape_ids"][0].clone());
    call(
        editor,
        context,
        peer,
        "set_selection_mode",
        json!({"scene_token":token,"mode":"vertex"}),
    )
    .await;
    let target = json!({"node_id":node,"shape_id":found["shape_ids"][0],"instance_path":full,"vertex_index":vertex});
    let picked = call(
        editor,
        context,
        peer,
        "select_vertex",
        json!({"scene_token":token,"target":target}),
    )
    .await;
    assert_eq!(picked.is_error, Some(false));
    Setup {
        token,
        node,
        path,
        target,
    }
}

async fn generation(editor: &mut Editor, context: &egui::Context, peer: &Peer<RoleClient>) -> u64 {
    call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["selection_generation"]
        .as_u64()
        .unwrap()
}

fn begin_args(setup: &Setup, generation: Option<u64>) -> Value {
    let mut args = json!({
        "scene_token":setup.token,"node_id":setup.node,"instance_path":setup.path,
        "kind":"move","axis":"x"
    });
    if let Some(generation) = generation {
        args["expected_selection_generation"] = json!(generation);
    }
    args
}

fn code(result: CallToolResult) -> Value {
    result.structured_content.unwrap()["error"]["code"].clone()
}

fn vertex_x(editor: &Editor, vertex: usize) -> f64 {
    editor.history.scene().shapes()[0]
        .mesh()
        .effective_point_cm(vertex)
        .unwrap()[0]
}

#[tokio::test]
async fn sdk_remote_component_move_matches_a_native_drag_and_is_one_undo_step() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let peer = client.peer();
    let setup = cube_in_vertex_mode(&mut editor, &context, peer, 3).await;
    let before = editor.history.scene().clone();
    let at = generation(&mut editor, &context, peer).await;
    let begin = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(begin["outcome"], "manipulation_started");
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    for distance in [4.0, 2.5] {
        let updated = call(
            &mut editor,
            &context,
            peer,
            "update_manipulation",
            json!({"scene_token":setup.token,"manipulation_id":id,"distance":distance}),
        )
        .await;
        assert_eq!(updated.is_error, Some(false));
    }
    for vertex in 0..8 {
        let want = before.shapes()[0]
            .mesh()
            .effective_point_cm(vertex)
            .unwrap()[0]
            + if vertex == 3 { 2.5 } else { 0.0 };
        assert!((vertex_x(&editor, vertex) - want).abs() < 1e-5, "{vertex}");
    }
    let active = call(&mut editor, &context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let block = &active["inspection"]["manipulation"]["active"];
    assert_eq!(
        (block["kind"].as_str(), block["axis"].as_str()),
        (Some("move"), Some("x"))
    );
    assert_eq!(block["provenance"], "remote");
    let finished = call(
        &mut editor,
        &context,
        peer,
        "finish_manipulation",
        json!({"scene_token":setup.token,"manipulation_id":id,"finish":"commit"}),
    )
    .await;
    assert_eq!(finished.is_error, Some(false));
    let committed = editor.history.scene().clone();
    // The same drag begun natively lands on the same bytes.
    let mut native = Editor::default();
    native.create(azteccraft::scene::Primitive::Cube);
    native
        .apply_operation(crate::editor_operations::Operation::SetSelectionMode(
            SelectionMode::Vertex,
        ))
        .unwrap();
    let owner = {
        let view = azteccraft::view_scene::ViewScene::from_native(native.history.scene(), 1 << 28)
            .unwrap();
        let i = &view.instances()[0];
        azteccraft::selection::ComponentOwner {
            transform: i.transform(),
            shape: i.shape(),
            path: i.path().to_vec(),
        }
    };
    native
        .apply_operation(
            crate::editor_operations::Operation::ModifyComponentSelection {
                kind: crate::editor_operations::ComponentKind::Vertex,
                mode: crate::editor_operations::MarqueeMode::Replace,
                owner,
                indices: vec![3],
            },
        )
        .unwrap();
    let target = native.selected_move_lead().unwrap();
    let reply = native
        .apply_operation(crate::editor_operations::Operation::BeginMove {
            target,
            axis: azteccraft::native_scene::WorldAxis::X,
            expected_selection_generation: None,
        })
        .unwrap();
    let crate::editor_operations::OperationReply::ManipulationStarted { manipulation_id } = reply
    else {
        panic!("{reply:?}");
    };
    let native_id = azteccraft::history::ManipulationId::from_decimal(&manipulation_id).unwrap();
    native
        .apply_operation(crate::editor_operations::Operation::UpdateMove {
            id: native_id,
            distance: crate::editor_operations::MoveDistance::Centimeters(2.5),
        })
        .unwrap();
    native
        .apply_operation(crate::editor_operations::Operation::FinishMove {
            id: native_id,
            finish: azteccraft::history::ManipulationFinish::Commit,
        })
        .unwrap();
    assert_eq!(
        native.history.scene().shapes()[0].mesh(),
        committed.shapes()[0].mesh()
    );
    let undone = call(
        &mut editor,
        &context,
        peer,
        "undo",
        json!({"scene_token":setup.token}),
    )
    .await;
    assert_eq!(undone.is_error, Some(false));
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_remote_component_move_begin_refuses_with_the_baseline_untouched() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let peer = client.peer();
    let setup = cube_in_vertex_mode(&mut editor, &context, peer, 3).await;
    let at = generation(&mut editor, &context, peer).await;
    let scene = editor.history.scene().clone();
    let expect_refused = |editor: &Editor| {
        assert!(editor.history.active_manipulation().is_none());
        assert_eq!(editor.history.scene(), &scene);
    };

    let missing = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, None),
    )
    .await;
    assert_eq!(missing.is_error, Some(true));
    let missing = missing.structured_content.unwrap();
    assert_eq!(missing["error"]["code"], "invalid_input");
    assert!(
        missing["error"]["message"]
            .as_str()
            .unwrap()
            .contains("expected_selection_generation")
    );
    expect_refused(&editor);

    let stale = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at.wrapping_sub(1))),
    )
    .await;
    assert_eq!(code(stale), "stale_selection");
    expect_refused(&editor);

    // A pick, mode switch or reselection since the inspection makes the generation stale.
    let other = json!({"node_id":setup.target["node_id"],"shape_id":setup.target["shape_id"],
        "instance_path":setup.target["instance_path"],"vertex_index":5});
    call(
        &mut editor,
        &context,
        peer,
        "select_vertex",
        json!({"scene_token":setup.token,"target":other}),
    )
    .await;
    let after_pick = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await;
    assert_eq!(code(after_pick), "stale_selection");
    expect_refused(&editor);
    let at = generation(&mut editor, &context, peer).await;

    let stale_token = call(&mut editor, &context, peer, "begin_manipulation", {
        let mut args = begin_args(&setup, Some(at));
        args["scene_token"] = json!(Uuid::nil().to_string());
        args
    })
    .await;
    assert_eq!(code(stale_token), "stale_scene");
    expect_refused(&editor);

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await;
    assert_eq!(code(draft), "busy_editing");
    assert_eq!(editor.name, "human_draft");
    expect_refused(&editor);
    editor.sync_fields();

    editor.request_close();
    let closing = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await;
    assert_eq!(code(closing), "busy_editing");
    expect_refused(&editor);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_active_component_move_is_busy_and_a_native_drag_cannot_be_updated_remotely() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let peer = client.peer();
    let setup = cube_in_vertex_mode(&mut editor, &context, peer, 3).await;
    let at = generation(&mut editor, &context, peer).await;
    let begin = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await
    .structured_content
    .unwrap();
    let id = begin["manipulation_id"].as_str().unwrap().to_owned();
    let preview = editor.history.scene().clone();
    let second = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        begin_args(&setup, Some(at)),
    )
    .await;
    assert_eq!(code(second), "busy_manipulation");
    for (name, args) in [
        ("undo", json!({"scene_token":setup.token})),
        (
            "set_selection_mode",
            json!({"scene_token":setup.token,"mode":"object"}),
        ),
    ] {
        let refused = call(&mut editor, &context, peer, name, args).await;
        assert_eq!(code(refused), "busy_manipulation", "{name}");
    }
    assert_eq!(editor.history.scene(), &preview);
    let cancelled = call(
        &mut editor,
        &context,
        peer,
        "finish_manipulation",
        json!({"scene_token":setup.token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;
    assert_eq!(cancelled.is_error, Some(false));
    assert!(editor.history.active_manipulation().is_none());
    assert!(vertex_x(&editor, 3).is_finite());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
