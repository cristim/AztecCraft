use super::*;

async fn rule(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
    rule: &str,
) -> Value {
    let token = editor.scene_token.to_string();
    call(
        editor,
        context,
        peer,
        "select_by_rule",
        json!({"scene_token":token,"rule":rule}),
    )
    .await
    .structured_content
    .unwrap()
}

async fn inspection(
    editor: &mut Editor,
    context: &egui::Context,
    peer: &Peer<RoleClient>,
) -> Value {
    call(editor, context, peer, "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]
        .clone()
}

fn reported(inspection: &Value) -> Vec<String> {
    inspection["selection"]
        .as_array()
        .unwrap()
        .iter()
        .map(|member| member["node"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn sdk_select_by_rule_all_and_inverse_report_the_ordered_set_with_the_lead_last() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for _ in 0..3 {
        editor.create(Primitive::Cube);
    }
    let targets = editor.history.scene().transform_paths().unwrap();
    editor.select(Some(targets[0].node));
    frame(&mut editor, &context, vec![]);
    let ids: Vec<_> = targets.iter().map(|t| t.node.to_decimal()).collect();

    let reply = rule(&mut editor, &context, client.peer(), "all").await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(editor.selection.members(), targets);
    let seen = inspection(&mut editor, &context, client.peer()).await;
    assert_eq!(seen["selection_count"], 3);
    assert_eq!(reported(&seen), ids);
    assert_eq!(seen["selected_node"], ids[2]);
    let again = rule(&mut editor, &context, client.peer(), "all").await;
    assert_eq!(again["outcome"], "unchanged");

    editor.select(Some(targets[0].node));
    frame(&mut editor, &context, vec![]);
    let reply = rule(&mut editor, &context, client.peer(), "inverse").await;
    assert_eq!(reply["outcome"], "changed");
    let seen = inspection(&mut editor, &context, client.peer()).await;
    assert_eq!(reported(&seen), [ids[1].clone(), ids[2].clone()]);
    let reply = rule(&mut editor, &context, client.peer(), "inverse").await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        reported(&inspection(&mut editor, &context, client.peer()).await),
        [ids[0].clone()]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_by_rule_hierarchy_adds_descendants_on_an_opened_group() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("grouped.mb");
    std::fs::write(&source, super::selection::grouped_scene_bytes()).unwrap();
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
    let paths = editor.history.scene().transform_paths().unwrap();
    assert_eq!(paths.len(), 2);
    editor.select(Some(paths[0].node));
    frame(&mut editor, &context, vec![]);
    let reply = rule(&mut editor, &context, client.peer(), "hierarchy").await;
    assert_eq!(reply["outcome"], "changed");
    assert_eq!(
        editor.selection.members(),
        [paths[1].clone(), paths[0].clone()]
    );
    assert_eq!(
        reported(&inspection(&mut editor, &context, client.peer()).await),
        [paths[1].node.to_decimal(), paths[0].node.to_decimal()]
    );
    let reply = rule(&mut editor, &context, client.peer(), "hierarchy").await;
    assert_eq!(reply["outcome"], "unchanged");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_select_by_rule_rejects_bad_arguments_and_reports_guard_refusals() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    editor.deselect_all();
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    for arguments in [
        json!({"scene_token":token}),
        json!({"scene_token":token,"rule":"similar"}),
        json!({"scene_token":token,"rule":"all","mode":"object"}),
        json!({"rule":"all"}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "select_by_rule",
                arguments
            )
            .await
            .is_err()
        );
    }
    assert!(editor.selection.is_empty());
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "select_by_rule",
        json!({"scene_token":Uuid::new_v4().to_string(),"rule":"all"}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    editor.select(
        editor
            .history
            .scene()
            .transform_paths()
            .unwrap()
            .first()
            .map(|t| t.node),
    );
    editor.channels[0] = "9".into();
    editor.channel_intents[0] = true;
    let before = editor.selection.clone();
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "select_by_rule",
        json!({"scene_token":token,"rule":"all"}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.selection, before);
    editor.channel_intents[0] = false;
    editor.sync_fields();
    let mode = call(
        &mut editor,
        &context,
        client.peer(),
        "set_selection_mode",
        json!({"scene_token":token,"mode":"face"}),
    )
    .await;
    assert_ne!(mode.is_error, Some(true));
    for rule_name in ["all", "hierarchy", "inverse"] {
        let refused = call(
            &mut editor,
            &context,
            client.peer(),
            "select_by_rule",
            json!({"scene_token":token,"rule":rule_name}),
        )
        .await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "invalid_input",
            "{rule_name}"
        );
        assert_eq!(editor.selection, before, "{rule_name}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
