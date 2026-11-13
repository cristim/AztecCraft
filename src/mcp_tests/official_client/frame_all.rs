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
    let id = editor.selected().unwrap();
    let node = editor
        .history
        .scene()
        .nodes()
        .iter()
        .find(|node| node.id() == id)
        .unwrap();
    let mut transform = editor.transform_input(node.transform()).unwrap();
    transform.translation[0] = 20.0;
    editor
        .apply_operation(Operation::SetTransform { id, transform })
        .unwrap();
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let selected = editor.selected();
    let view = azteccraft::view_scene::ViewScene::from_native(&scene, 256 * 1024 * 1024).unwrap();
    let mut expected = editor.camera.clone();
    expected.frame_view(&view, None, editor.aspect).unwrap();
    assert_eq!(view.instances().len(), 2);
    for instance in view.instances() {
        let mut single = editor.camera.clone();
        single
            .frame_view(&view, Some(instance.path()), editor.aspect)
            .unwrap();
        assert_ne!(expected, single);
    }
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
        assert_eq!(editor.selected(), selected);
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

#[tokio::test]
async fn sdk_frame_all_fit_factor_matches_the_native_operation_and_refuses_bad_values() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let start_camera = editor.camera.clone();
    let token = editor.scene_token.to_string();
    let mut native = editor.camera.clone();
    editor.native_operation(Operation::FrameAll {
        fit_factor: Some(0.5),
    });
    native = std::mem::replace(&mut editor.camera, native);
    let args = json!({"scene_token": token, "fit_factor": 0.5});
    let reply = call(&mut editor, &context, client.peer(), "frame_all", args).await;
    assert_eq!(reply.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.camera, native);
    let fitted = editor.camera.distance();
    editor.camera = start_camera.clone();
    let args = json!({"scene_token": token});
    call(&mut editor, &context, client.peer(), "frame_all", args).await;
    assert!(fitted > editor.camera.distance());
    editor.camera = start_camera.clone();
    for bad in [
        json!(0),
        json!(-1),
        json!(1.5),
        json!(0.009),
        json!(1e-10),
        json!(1e-30),
    ] {
        let args = json!({"scene_token": token, "fit_factor": bad});
        let refused = call(&mut editor, &context, client.peer(), "frame_all", args).await;
        assert_eq!(
            refused.structured_content.unwrap()["error"]["code"],
            "invalid_input",
            "{bad}"
        );
        assert_eq!(editor.camera, start_camera, "{bad}");
    }
    let args = json!({"scene_token": token, "fit_factor": "half"});
    assert!(
        call_response(&mut editor, &context, client.peer(), "frame_all", args)
            .await
            .is_err()
    );
    let args = json!({"scene_token": Uuid::new_v4().to_string(), "fit_factor": 0.5});
    let stale = call(&mut editor, &context, client.peer(), "frame_all", args).await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.camera, start_camera);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
