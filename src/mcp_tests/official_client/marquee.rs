use super::*;
use azteccraft::native_scene::{NativeScene, NativeTransform};
use azteccraft::viewport::NdcRect;

fn code(result: rmcp::model::CallToolResult) -> String {
    result.structured_content.unwrap()["error"]["code"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Three cubes at x = -2, 0 and 2 in a fresh document of the editor, nothing selected.
fn three_cubes(editor: &mut Editor) -> Vec<azteccraft::native_scene::TransformTarget> {
    let mut scene = NativeScene::default();
    for (index, x) in [-2.0, 0.0, 2.0].into_iter().enumerate() {
        let node = scene.create(Primitive::Cube, &format!("c{index}")).unwrap();
        scene
            .set_transform(
                node,
                NativeTransform {
                    translation: [x, 0.0, 0.0],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    editor.history = azteccraft::history::History::new(scene);
    editor.sync_fields();
    editor.history.scene().transform_paths().unwrap()
}

async fn marquee(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    rect: Value,
    mode: Option<&str>,
) -> Value {
    let token = editor.scene_token.to_string();
    let mut arguments = json!({"scene_token":token,"rect":rect});
    if let Some(mode) = mode {
        arguments["mode"] = json!(mode);
    }
    call(editor, context, peer, "select_in_rect", arguments)
        .await
        .structured_content
        .unwrap()
}

fn whole() -> Value {
    json!({"min":[-1,-1],"max":[1,1]})
}

fn ids(targets: &[azteccraft::native_scene::TransformTarget]) -> Vec<String> {
    targets.iter().map(|t| t.node.to_decimal()).collect()
}

async fn reported(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> Vec<String> {
    let seen = call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    seen["inspection"]["selection"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["node"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn sdk_select_in_rect_is_listed_with_a_closed_schema_and_not_idempotent() {
    let context = egui::Context::default();
    let (_editor, client, server) = start(&context).await;
    let listed = client.peer().list_tools(None).await.unwrap();
    let tool = listed
        .tools
        .iter()
        .find(|tool| tool.name == "select_in_rect")
        .expect("select_in_rect is listed");
    let schema = Value::Object((*tool.input_schema).clone());
    assert_eq!(schema["additionalProperties"], json!(false));
    assert_eq!(schema["required"], json!(["scene_token", "rect"]));
    let annotations = tool.annotations.as_ref().unwrap();
    assert_eq!(annotations.idempotent_hint, Some(false));
    assert_eq!(annotations.read_only_hint, Some(false));
    client.cancel().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_in_rect_replace_add_remove_and_toggle_report_the_ordered_set() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let targets = three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    let all = ids(&targets);
    let peer = client.peer();

    let reply = marquee(&mut editor, &context, peer, whole(), None).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(reported(&mut editor, &context, peer).await, all);
    assert_eq!(editor.selection.lead(), targets.last());
    let generation = editor.selection.generation();
    let again = marquee(&mut editor, &context, peer, whole(), Some("replace")).await;
    assert_eq!(again["outcome"], "unchanged");
    assert_eq!(editor.selection.generation(), generation);

    // Inverted corners select the same rectangle; a rect around the first cube alone.
    let first = editor
        .camera
        .project(glam::Vec3::new(-2.0, 0.0, 0.0), editor.aspect)
        .unwrap()
        .unwrap();
    let around = |half: f32| json!({"min":[first.x + half, first.y + half],"max":[first.x - half, first.y - half]});
    let reply = marquee(&mut editor, &context, peer, around(0.01), Some("remove")).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        reported(&mut editor, &context, peer).await,
        [all[1].clone(), all[2].clone()]
    );
    let reply = marquee(&mut editor, &context, peer, around(0.01), Some("add")).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        reported(&mut editor, &context, peer).await,
        [all[1].clone(), all[2].clone(), all[0].clone()]
    );
    let reply = marquee(&mut editor, &context, peer, around(0.01), Some("toggle")).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        reported(&mut editor, &context, peer).await,
        [all[1].clone(), all[2].clone()]
    );
    // Remove leaves an absent hit alone, where toggle would add it.
    let reply = marquee(&mut editor, &context, peer, around(0.01), Some("remove")).await;
    assert_eq!(reply["outcome"], "unchanged");
    assert_eq!(
        reported(&mut editor, &context, peer).await,
        [all[1].clone(), all[2].clone()]
    );
    // Empty space: add, remove and toggle change nothing, replace clears.
    let empty = json!({"min":[0.99,0.99],"max":[1,1]});
    for mode in ["add", "remove", "toggle"] {
        let reply = marquee(&mut editor, &context, peer, empty.clone(), Some(mode)).await;
        assert_eq!(reply["outcome"], "unchanged", "{mode}");
    }
    let reply = marquee(&mut editor, &context, peer, empty, Some("replace")).await;
    assert_eq!(reply["outcome"], "changed");
    assert!(reported(&mut editor, &context, peer).await.is_empty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// Omitting `mode` means replace: with a non-empty selection, a rectangle that hits one other
/// object leaves exactly that object, not the union (an `add` default would keep all three).
#[tokio::test]
async fn sdk_select_in_rect_without_a_mode_replaces_a_non_empty_selection() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let targets = three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    let all = ids(&targets);
    let peer = client.peer();

    let reply = marquee(&mut editor, &context, peer, whole(), None).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(reported(&mut editor, &context, peer).await, all);

    let first = editor
        .camera
        .project(glam::Vec3::new(-2.0, 0.0, 0.0), editor.aspect)
        .unwrap()
        .unwrap();
    let around =
        json!({"min":[first.x - 0.01, first.y - 0.01],"max":[first.x + 0.01, first.y + 0.01]});
    let reply = marquee(&mut editor, &context, peer, around, None).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        reported(&mut editor, &context, peer).await,
        [all[0].clone()]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// The same rectangle run through the native operation and through the wire, from the same
/// document, camera and aspect, leaves the same selection and generation.
#[tokio::test]
async fn sdk_select_in_rect_matches_the_native_operation_at_one_camera() {
    use crate::editor_operations::MarqueeMode;
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let targets = three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    editor.select(Some(targets[2].node));
    frame(&mut editor, &context, vec![]);
    let baseline = (editor.selection.clone(), editor.selection.generation());
    let rect = json!({"min":[-1,-1],"max":[0.1,1]});
    let native_rect = NdcRect::new(glam::Vec2::new(-1.0, -1.0), glam::Vec2::new(0.1, 1.0)).unwrap();
    editor.native_operation(Operation::SelectInRect {
        rect: native_rect,
        mode: MarqueeMode::Toggle,
    });
    let native = (
        editor.selection.members().to_vec(),
        editor.selection.generation(),
    );
    editor.selection = baseline.0.clone();
    editor.sync_fields();
    let reply = marquee(&mut editor, &context, client.peer(), rect, Some("toggle")).await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(editor.selection.members(), native.0);
    assert_eq!(editor.selection.generation(), native.1);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_in_rect_rejects_malformed_arguments_without_changing_anything() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let bad = [
        json!({"scene_token":token}),
        json!({"rect":whole()}),
        json!({"scene_token":token,"rect":{"min":[-1,-1]}}),
        json!({"scene_token":token,"rect":{"min":[-1],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1,0],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1.5,-1],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1],"max":[1,1.0001]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1],"max":[1.00000001,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1],"max":[1,1.00000001]}}),
        json!({"scene_token":token,"rect":{"min":[-1.00000001,-1],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1.00000001],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[-1,-1],"max":[1,1],"extra":1}}),
        json!({"scene_token":token,"rect":whole(),"mode":"invert"}),
        json!({"scene_token":token,"rect":whole(),"unknown":true}),
        json!({"scene_token":token,"rect":{"min":["a",0],"max":[1,1]}}),
        json!({"scene_token":token,"rect":{"min":[null,0],"max":[1,1]}}),
    ];
    for arguments in bad {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "select_in_rect",
                arguments.clone()
            )
            .await
            .is_err(),
            "{arguments}"
        );
    }
    assert!(editor.selection.is_empty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_in_rect_range_applies_to_the_wire_value_not_the_f32() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for rect in [
        json!({"min":[-1.0,-1.0],"max":[1.0,1.0]}),
        json!({"min":[1.0,1.0],"max":[-1.0,-1.0]}),
    ] {
        let reply = marquee(&mut editor, &context, client.peer(), rect, None).await;
        assert!(reply.get("error").is_none(), "{reply}");
    }
    editor.selection.clear();
    let before = editor.scene_token;
    for rect in [
        json!({"min":[-1,-1],"max":[1.00000001,1]}),
        json!({"min":[-1,-1.00000001],"max":[1,1]}),
    ] {
        let result = call_response(
            &mut editor,
            &context,
            client.peer(),
            "select_in_rect",
            json!({"scene_token":token,"rect":rect}),
        )
        .await;
        assert!(result.is_err(), "{rect}");
    }
    assert!(editor.selection.is_empty());
    assert_eq!(editor.scene_token, before);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_in_rect_reports_the_shared_guard_refusals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let targets = three_cubes(&mut editor);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let peer = client.peer();
    let args = |token: &str| json!({"scene_token":token,"rect":whole()});

    let stale = call(
        &mut editor,
        &context,
        peer,
        "select_in_rect",
        args(&Uuid::new_v4().to_string()),
    )
    .await;
    assert_eq!(code(stale), "stale_scene");
    assert!(editor.selection.is_empty());

    editor.select(Some(targets[0].node));
    frame(&mut editor, &context, vec![]);
    let before = editor.selection.clone();
    editor.channels[0] = "9".into();
    editor.channel_intents[0] = true;
    let busy = call(&mut editor, &context, peer, "select_in_rect", args(&token)).await;
    assert_eq!(code(busy), "busy_editing");
    assert_eq!(editor.selection, before);
    editor.channel_intents[0] = false;
    editor.sync_fields();

    let node = targets[0].node.to_decimal();
    let path: Vec<String> = targets[0].path.iter().map(|id| id.to_decimal()).collect();
    let begun = call(
        &mut editor,
        &context,
        peer,
        "begin_manipulation",
        json!({"scene_token":token,"kind":"move","node_id":node,"instance_path":path,"axis":"x"}),
    )
    .await
    .structured_content
    .unwrap();
    let id = begun["manipulation_id"].as_str().unwrap().to_owned();
    let busy = call(&mut editor, &context, peer, "select_in_rect", args(&token)).await;
    assert_eq!(code(busy), "busy_manipulation");
    assert_eq!(editor.selection, before);
    call(
        &mut editor,
        &context,
        peer,
        "finish_manipulation",
        json!({"scene_token":token,"manipulation_id":id,"finish":"cancel"}),
    )
    .await;

    for mode in ["face", "vertex", "edge"] {
        call(
            &mut editor,
            &context,
            peer,
            "set_selection_mode",
            json!({"scene_token":token,"mode":mode}),
        )
        .await;
        let refused = call(&mut editor, &context, peer, "select_in_rect", args(&token)).await;
        assert_eq!(code(refused), "invalid_input", "{mode}");
        assert_eq!(editor.selection, before, "{mode}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
