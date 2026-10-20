use super::*;

/// MCP primitive, options, then expected points, edges and faces (closed forms: Euler 2).
const BUILDS: [(&str, &str, [u64; 3]); 6] = [
    (
        "cube",
        r#"{"width":2,"height":3,"depth":4,"subdivisions_width":2,"subdivisions_height":3,"subdivisions_depth":4,"axis":"x"}"#,
        [54, 104, 52],
    ),
    (
        "plane",
        r#"{"width":5,"height":0.5,"subdivisions_width":3,"subdivisions_height":4,"axis":"z"}"#,
        [20, 31, 12],
    ),
    (
        "sphere",
        r#"{"radius":2.5,"subdivisions_axis":8,"subdivisions_height":5,"axis":"z"}"#,
        [34, 72, 40],
    ),
    (
        "cylinder",
        r#"{"radius":0.25,"height":10,"subdivisions_axis":6,"subdivisions_height":3,"axis":"x"}"#,
        [24, 42, 20],
    ),
    (
        "cone",
        r#"{"radius":3,"height":1e6,"subdivisions_axis":6,"subdivisions_height":3,"axis":"y"}"#,
        [19, 36, 19],
    ),
    // An empty options object is the defaults, like omitting it.
    ("cube", "{}", [8, 12, 6]),
];

fn counts(inspected: &Value) -> Vec<[u64; 3]> {
    inspected["inspection"]["shapes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|shape| ["points", "edges", "faces"].map(|key| shape[key].as_u64().unwrap()))
        .collect()
}

async fn inspect(editor: &mut Editor, context: &egui::Context, client: &Peer<RoleClient>) -> Value {
    call(editor, context, client, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn optioned_create_builds_the_requested_mesh_and_undoes_through_the_shared_editor() {
    for (primitive, options, expected) in BUILDS {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let token = editor.scene_token.to_string();
        let options: Value = serde_json::from_str(options).unwrap();
        let before = editor.history.scene().clone();
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await
        .structured_content
        .unwrap();
        assert_eq!(created["outcome"], "created", "{primitive}");
        assert_eq!(
            counts(&inspect(&mut editor, &context, client.peer()).await),
            vec![expected],
            "{primitive} {options}"
        );
        call(
            &mut editor,
            &context,
            client.peer(),
            "undo",
            json!({"scene_token":token}),
        )
        .await;
        assert_eq!(editor.history.scene(), &before);
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn calls_without_options_keep_the_default_meshes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    for (primitive, expected) in [
        ("cube", [8, 12, 6]),
        ("plane", [4, 4, 1]),
        ("sphere", [382, 780, 400]),
        ("cylinder", [40, 60, 22]),
        ("cone", [21, 40, 21]),
    ] {
        let reply = call(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive}),
        )
        .await;
        assert_eq!(reply.is_error, Some(false), "{primitive}");
        let shapes = counts(&inspect(&mut editor, &context, client.peer()).await);
        assert_eq!(shapes.last(), Some(&expected), "{primitive}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn invalid_or_mismatched_options_are_invalid_params_and_leave_the_editor_unchanged() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let selected = (editor.selected, editor.selected_path.clone());
    let cases = [
        ("cube", json!(null)),
        ("cube", json!([])),
        ("cube", json!({"width":null})),
        ("cube", json!({"axis":null})),
        ("cube", json!({"width":0})),
        ("cube", json!({"width":-1})),
        ("cube", json!({"width":1e-7})),
        ("cube", json!({"width":1000001})),
        ("cube", json!({"width":"2"})),
        ("cube", json!({"axis":"w"})),
        ("cube", json!({"axis":"X"})),
        ("cube", json!({"subdivisions_width":0})),
        ("cube", json!({"subdivisions_width":129})),
        ("cube", json!({"subdivisions_width":3.0})),
        ("cube", json!({"subdivisions_width":2.5})),
        ("cube", json!({"subdivisions_width":-1})),
        ("cube", json!({"subdivisions_width":4294967296u64})),
        ("cube", json!({"subdivisions_width":"3"})),
        // Over the face budget although every field is in range.
        (
            "cube",
            json!({"subdivisions_width":128,"subdivisions_height":128,"subdivisions_depth":128}),
        ),
        ("cube", json!({"depht":1})),
        ("cube", json!({"radius":1})),
        ("plane", json!({"depth":1})),
        ("plane", json!({"subdivisions_depth":2})),
        ("sphere", json!({"width":1})),
        ("sphere", json!({"height":1})),
        ("sphere", json!({"subdivisions_axis":2})),
        ("sphere", json!({"subdivisions_height":1})),
        ("cylinder", json!({"width":1})),
        ("cylinder", json!({"subdivisions_axis":2})),
        ("cylinder", json!({"subdivisions_height":0})),
        ("cone", json!({"depth":1})),
        ("cone", json!({"radius":0})),
        ("cone", json!({"subdivisions_height":129})),
    ];
    for (primitive, options) in cases {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            json!({"scene_token":token,"primitive":primitive,"options":options}),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {primitive} {options}");
        };
        assert_eq!(
            error.code,
            rmcp::model::ErrorCode::INVALID_PARAMS,
            "{primitive} {options}"
        );
        assert_eq!(editor.history.scene(), &scene, "{primitive} {options}");
        assert_eq!(editor.scene_token.to_string(), token);
        assert_eq!((editor.selected, editor.selected_path.clone()), selected);
        assert!(!editor.history.can_redo());
    }
    for arguments in [
        json!({"scene_token":token,"primitive":"torus"}),
        json!({"scene_token":token,"primitive":"cube","extra":1}),
        json!({"scene_token":token,"options":{}}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "create_primitive",
            arguments.clone(),
        )
        .await;
        assert!(
            matches!(
                rejected,
                Err(rmcp::ServiceError::McpError(ref error))
                    if error.code == rmcp::model::ErrorCode::INVALID_PARAMS
            ),
            "{arguments}"
        );
        assert_eq!(editor.history.scene(), &scene);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn optioned_create_obeys_the_scene_token_draft_modal_and_close_guards() {
    let options = json!({"width":2,"subdivisions_width":3});
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":Uuid::new_v4().to_string(),"primitive":"cube","options":options}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.history.scene(), &before);

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","options":options}),
    )
    .await;
    assert_eq!(
        draft.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.name, "human_draft");
    assert_eq!(editor.history.scene(), &before);
    editor.sync_fields();

    let pending = call(
        &mut editor,
        &context,
        client.peer(),
        "new_scene",
        json!({"scene_token":token}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(pending["outcome"], "pending_confirmation");
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","options":options}),
    )
    .await;
    assert_eq!(
        modal.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();

    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let before = editor.history.scene().clone();
    editor.request_close();
    let closing = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube","options":options}),
    )
    .await;
    assert_eq!(
        closing.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn optioned_create_on_an_opened_maya_binary_is_refused_without_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.mb");
    std::fs::write(&path, binary_fixture::specimen()).unwrap();
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let original = editor.scene_token.to_string();
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":original,"path":path.to_str().unwrap()}),
    )
    .await
    .structured_content
    .unwrap();
    let token = opened["scene_token"].as_str().unwrap().to_owned();
    let baseline = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"sphere","options":{"radius":2}}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "scene_error"
    );
    assert_eq!(editor.history.scene(), &baseline);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
