//! Exercises the authored ASCII path through the actual Editor and official SDK.
use super::*;

async fn tool(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    name: &'static str,
    mut arguments: Value,
) -> Value {
    arguments["scene_token"] = json!(editor.scene_token.to_string());
    call(editor, context, client.peer(), name, arguments)
        .await
        .structured_content
        .unwrap()
}

#[tokio::test]
async fn authored_ascii_sdk_save_as_edit_save_and_history_preserve_document() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.ma");
    let created = tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    assert_eq!(created["outcome"], "created");
    let saved = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path":path}),
    )
    .await;
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(saved["dirty"], false);
    let first = std::fs::read(&path).unwrap();
    assert!(first.starts_with(b"//Maya ASCII 2026 scene"));
    let id = editor.history.scene().nodes()[0].id().to_decimal();
    let edited = tool(&mut editor, &context, &client, "set_transform", json!({"node_id":id,"translation":[2.5,0.0,0.0],"rotation_degrees":[0.0,0.0,0.0],"scale":[1.0,1.0,1.0]})).await;
    assert_eq!(edited["outcome"], "changed");
    let saved = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(saved["outcome"], "saved");
    assert!(!editor.is_dirty());
    let second = std::fs::read(&path).unwrap();
    assert_ne!(first, second);
    assert!(String::from_utf8(second.clone()).unwrap().contains("2.5"));
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(editor.is_dirty());
    tool(&mut editor, &context, &client, "redo", json!({})).await;
    assert!(!editor.is_dirty());
    let saved = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(saved["outcome"], "saved");
    assert_eq!(std::fs::read(&path).unwrap(), second);
    let inspection = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(inspection["inspection"]["document_origin"], "maya_ascii");
    assert_eq!(inspection["inspection"]["has_file"], true);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn authored_ascii_sdk_material_refusal_preserves_dirty_scene_and_destinations() {
    for already_saved in [false, true] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.ma");
        let created = tool(
            &mut editor,
            &context,
            &client,
            "create_primitive",
            json!({"primitive":"cube"}),
        )
        .await;
        assert_eq!(created["outcome"], "created");
        let original = if already_saved {
            let saved = tool(
                &mut editor,
                &context,
                &client,
                "save_scene_as",
                json!({"path":path}),
            )
            .await;
            assert_eq!(saved["outcome"], "saved");
            Some(std::fs::read(&path).unwrap())
        } else {
            None
        };
        let shape = editor.history.scene().shapes()[0].id().to_decimal();
        let assigned = tool(
            &mut editor,
            &context,
            &client,
            "assign_session_material",
            json!({"shape_id":shape,"color":[1.0,0.0,0.0]}),
        )
        .await;
        assert_eq!(assigned["material_id"], "1");
        assert!(editor.is_dirty());
        let scene = editor.history.scene().clone();
        let undo = editor.history.can_undo();
        let redo = editor.history.can_redo();
        let token = editor.scene_token;
        let error = if already_saved {
            tool(&mut editor, &context, &client, "save_scene", json!({})).await
        } else {
            tool(
                &mut editor,
                &context,
                &client,
                "save_scene_as",
                json!({"path":path}),
            )
            .await
        };
        assert_eq!(error["error"]["code"], "save_ascii_write");
        assert!(editor.is_dirty());
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.history.can_undo(), undo);
        assert_eq!(editor.history.can_redo(), redo);
        assert_eq!(editor.scene_token, token);
        match original {
            Some(bytes) => assert_eq!(std::fs::read(&path).unwrap(), bytes),
            None => assert!(!path.exists()),
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn authored_ascii_sdk_open_and_inspect_show_the_mesh_and_refuse_a_hand_edit() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("authored.ma");
    tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    let saved = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path":path}),
    )
    .await;
    assert_eq!(saved["outcome"], "saved");
    let bytes = std::fs::read(&path).unwrap();

    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":path}),
    )
    .await;
    assert_eq!(opened["outcome"], "document_applied");
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
    let inspection = &inspected["inspection"];
    assert_eq!(inspection["document_origin"], "maya_ascii");
    assert_eq!(inspection["shapes"].as_array().unwrap().len(), 1);
    assert_eq!(inspection["curves"].as_array().unwrap().len(), 0);
    assert_eq!(inspection["save_available"], true);
    assert_eq!(inspection["save_as_available"], true);
    assert!(inspection["read_only_reason"].is_null());
    let node = &inspection["nodes"][0];
    let selected = tool(
        &mut editor,
        &context,
        &client,
        "select_node",
        json!({"node_id":node["id"],"instance_path":node["instance_paths"][0]}),
    )
    .await;
    assert_eq!(selected["outcome"], "changed");
    assert_eq!(
        editor.selected().unwrap().to_decimal(),
        node["id"].as_str().unwrap()
    );

    let renamed = tool(
        &mut editor,
        &context,
        &client,
        "rename_node",
        json!({"node_id":node["id"],"name":"renamed"}),
    )
    .await;
    assert_eq!(renamed["outcome"], "changed");
    assert!(editor.is_dirty());
    tool(&mut editor, &context, &client, "undo", json!({})).await;
    assert!(!editor.is_dirty());

    let mut edited = bytes.clone();
    edited.extend_from_slice(b"// hand edit\n");
    let hand = directory.path().join("hand.ma");
    std::fs::write(&hand, &edited).unwrap();
    let refused = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":hand}),
    )
    .await;
    assert!(refused["error"].is_object(), "{refused}");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn inspect(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

/// Names and translations in node order: the value half of the reopen oracle (ids may be
/// renumbered by a reopen).
fn node_values(inspection: &Value) -> Vec<(Value, Value)> {
    inspection["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|node| {
            (
                node["name"].clone(),
                node["transform"]["translation"].clone(),
            )
        })
        .collect()
}

/// Builds the tool call from the inspection of the freshly opened document.
type Edit = fn(&Value) -> Vec<(&'static str, Value)>;

#[derive(Debug, PartialEq)]
enum Expect {
    Saves,
    /// The edit itself is refused on an authored scene, so there is nothing to save.
    EditRefused,
    Refuses(&'static str),
}

/// One MCP edit applied to a freshly opened authored `.ma`: it must land in exactly one of
/// "Save writes a file that reopens through the gate with the same bytes and values" or
/// "Save refuses with a typed code and the document stays dirty".
async fn round_trip(label: &str, edit: Edit, expect: Expect) {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("doc.ma");
    tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    let saved = tool(
        &mut editor,
        &context,
        &client,
        "save_scene_as",
        json!({"path":path}),
    )
    .await;
    assert_eq!(saved["outcome"], "saved", "{label}");
    let opened = tool(
        &mut editor,
        &context,
        &client,
        "open_scene",
        json!({"path":path}),
    )
    .await;
    assert_eq!(opened["outcome"], "document_applied", "{label}");
    let before = inspect(&mut editor, &context, &client).await;
    let mut manipulation = Value::Null;
    for (name, mut args) in edit(&before) {
        if args["manipulation_id"].is_null() && args.get("manipulation_id").is_some() {
            args["manipulation_id"] = manipulation.clone();
        }
        let edited = tool(&mut editor, &context, &client, name, args).await;
        if expect == Expect::EditRefused {
            assert_eq!(edited["error"]["code"], "scene_error", "{label}: {edited}");
            assert!(!editor.is_dirty(), "{label}");
            client.cancel().await.unwrap();
            drop(editor);
            server.await.unwrap();
            return;
        }
        assert!(edited["error"].is_null(), "{label}: {edited}");
        if !edited["manipulation_id"].is_null() {
            manipulation = edited["manipulation_id"].clone();
        }
    }
    assert!(editor.is_dirty(), "{label}");
    let original = std::fs::read(&path).unwrap();
    let result = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    match expect {
        Expect::Refuses(code) => {
            assert_eq!(result["error"]["code"], code, "{label}: {result}");
            assert!(editor.is_dirty(), "{label}");
            assert_eq!(std::fs::read(&path).unwrap(), original, "{label}");
        }
        Expect::EditRefused => unreachable!(),
        Expect::Saves => {
            assert_eq!(result["outcome"], "saved", "{label}: {result}");
            assert!(!editor.is_dirty(), "{label}");
            let written = std::fs::read(&path).unwrap();
            let values = node_values(&inspect(&mut editor, &context, &client).await);
            let reopened = tool(
                &mut editor,
                &context,
                &client,
                "open_scene",
                json!({"path":path}),
            )
            .await;
            assert_eq!(reopened["outcome"], "document_applied", "{label}");
            let after = inspect(&mut editor, &context, &client).await;
            assert_eq!(
                after["save_available"], true,
                "{label}: reopened through the gate"
            );
            assert_eq!(node_values(&after), values, "{label}");
            let copy = directory.path().join("copy.ma");
            let copied = tool(
                &mut editor,
                &context,
                &client,
                "save_scene_as",
                json!({"path":copy}),
            )
            .await;
            assert_eq!(copied["outcome"], "saved", "{label}");
            assert_eq!(std::fs::read(&copy).unwrap(), written, "{label}");
        }
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

fn first_node(inspection: &Value) -> Value {
    inspection["nodes"][0]["id"].clone()
}

#[tokio::test]
async fn authored_ascii_sdk_every_edit_either_round_trips_or_refuses_save() {
    use Expect::{EditRefused, Refuses, Saves};
    let cases: Vec<(&str, Edit, Expect)> = vec![
        (
            "create cube",
            |_| vec![("create_primitive", json!({"primitive":"cube"}))],
            Saves,
        ),
        (
            "create plane",
            |_| vec![("create_primitive", json!({"primitive":"plane"}))],
            Saves,
        ),
        (
            "create sphere",
            |_| vec![("create_primitive", json!({"primitive":"sphere"}))],
            Saves,
        ),
        (
            "create cylinder",
            |_| vec![("create_primitive", json!({"primitive":"cylinder"}))],
            Saves,
        ),
        (
            "create cone",
            |_| vec![("create_primitive", json!({"primitive":"cone"}))],
            Saves,
        ),
        (
            "rename",
            |i| {
                vec![(
                    "rename_node",
                    json!({"node_id":first_node(i),"name":"renamed"}),
                )]
            },
            Saves,
        ),
        (
            "set_transform",
            |i| {
                vec![(
                    "set_transform",
                    json!({"node_id":first_node(i),"translation":[2.5,-0.0,1e15],"rotation_degrees":[10.0,20.0,30.0],"scale":[1.0,2.0,3.0]}),
                )]
            },
            Saves,
        ),
        (
            "edit_node",
            |i| {
                vec![(
                    "edit_node",
                    json!({"node_id":first_node(i),"translation":[1.0,null,null]}),
                )]
            },
            Saves,
        ),
        (
            "extrude",
            |i| {
                let node = &i["nodes"][0];
                let shape = node["shape_ids"][0].clone();
                let mut path = node["instance_paths"][0].as_array().unwrap().clone();
                path.push(shape.clone());
                vec![(
                    "extrude_face",
                    json!({"node_id":node["id"],"shape_id":shape,"instance_path":path,"face_index":1,"distance":1.5}),
                )]
            },
            Saves,
        ),
        (
            "committed move",
            |i| {
                let node = &i["nodes"][0];
                vec![
                    (
                        "select_node",
                        json!({"node_id":node["id"],"instance_path":node["instance_paths"][0]}),
                    ),
                    (
                        "begin_manipulation",
                        json!({"node_id":node["id"],"instance_path":node["instance_paths"][0],"kind":"move","axis":"x"}),
                    ),
                    (
                        "update_manipulation",
                        json!({"manipulation_id":null,"distance":3.25}),
                    ),
                    (
                        "finish_manipulation",
                        json!({"manipulation_id":null,"finish":"commit"}),
                    ),
                ]
            },
            Saves,
        ),
        (
            "delete only node",
            |i| vec![("delete_node", json!({"node_id":first_node(i)}))],
            Saves,
        ),
        (
            "delete selected",
            |i| {
                let node = &i["nodes"][0];
                vec![
                    (
                        "select_node",
                        json!({"node_id":node["id"],"instance_path":node["instance_paths"][0]}),
                    ),
                    ("delete_selected", json!({})),
                ]
            },
            Saves,
        ),
        (
            "session material",
            |_| vec![("create_session_material", json!({"color":[0.1,0.2,0.3]}))],
            Refuses("save_ascii_write"),
        ),
        (
            "uvs",
            |i| {
                vec![(
                    "project_uv_planar",
                    json!({"shape_id":i["nodes"][0]["shape_ids"][0],"axis":"z"}),
                )]
            },
            Saves,
        ),
        (
            "material diffuse",
            |i| {
                vec![(
                    "set_material_diffuse",
                    json!({"shape_id":i["nodes"][0]["shape_ids"][0],"value":0.5}),
                )]
            },
            EditRefused,
        ),
    ];
    for (label, edit, expect) in cases {
        round_trip(label, edit, expect).await;
    }
}

#[cfg(unix)]
#[tokio::test]
async fn authored_uncertain_save_forced_natively_is_inspected_and_reconciled_through_the_sdk() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("document");
    let moved = directory.path().join("moved");
    std::fs::create_dir(&parent).unwrap();
    let path = parent.join("new.ma");
    tool(
        &mut editor,
        &context,
        &client,
        "create_primitive",
        json!({"primitive":"cube"}),
    )
    .await;
    // The Uncertain step is forced by a native guarded call (the remote path has no fault
    // injection); only inspect_scene and save_scene run through the SDK, so token, draft and
    // modal guards are not exercised for that step.
    let relocated = std::cell::Cell::new(false);
    let guard = || {
        if !relocated.get()
            && std::fs::read_dir(&parent).unwrap().any(|entry| {
                entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".craft_tmp")
            })
        {
            std::fs::rename(&parent, &moved).unwrap();
            relocated.set(true);
        }
        true
    };
    assert!(matches!(
        editor
            .apply_operation_guarded(
                azteccraft::history::ManipulationProvenance::Native,
                Operation::SaveSceneAs { path: path.clone() },
                &guard
            )
            .unwrap(),
        OperationReply::SaveUncertain { dirty: true, .. }
    ));
    assert!(relocated.get());
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
        inspected["inspection"]["save_reconciliation_required"],
        true
    );
    assert_eq!(inspected["inspection"]["dirty"], true);
    let failed = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(failed["error"]["code"], "source_changed");
    assert!(editor.is_dirty() && editor.inspect(false).unwrap().save_reconciliation_required);
    std::fs::rename(&moved, &parent).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let reconciled = tool(&mut editor, &context, &client, "save_scene", json!({})).await;
    assert_eq!(reconciled["dirty"], false);
    assert!(!editor.is_dirty());
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
        inspected["inspection"]["save_reconciliation_required"],
        false
    );
    assert_eq!(inspected["inspection"]["has_file"], true);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
