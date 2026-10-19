use super::*;

#[tokio::test]
async fn sdk_frame_all_frames_the_whole_scene_and_refuses_an_empty_one() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let camera = editor.camera.clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "frame_all",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "nothing_to_frame"
    );
    assert_eq!(editor.camera, camera);

    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let selected = editor.selected;
    let view = azteccraft::view_scene::ViewScene::from_native(&scene, 256 * 1024 * 1024).unwrap();
    let mut expected = editor.camera.clone();
    expected.frame_view(&view, None, editor.aspect).unwrap();
    for outcome in ["changed", "unchanged"] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "frame_all",
            json!({"scene_token": token}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(editor.camera, expected);
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.selected, selected);
    }
    for arguments in [json!({}), json!({"scene_token": token, "extra": true})] {
        assert!(
            call_response(&mut editor, &context, client.peer(), "frame_all", arguments)
                .await
                .is_err()
        );
    }
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "frame_all",
        json!({"scene_token": Uuid::new_v4().to_string()}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    editor.name = "typing".into();
    editor.native_input_pending = true;
    let busy = call(
        &mut editor,
        &context,
        client.peer(),
        "frame_all",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(
        busy.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    editor.native_input_pending = false;
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
