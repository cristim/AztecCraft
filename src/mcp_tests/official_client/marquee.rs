use super::*;
use azteccraft::native_scene::{NativeScene, NativeTransform};

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
