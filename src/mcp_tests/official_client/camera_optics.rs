use super::*;
use azteccraft::{camera_optics::CameraOptics, native_scene::SceneCamera, viewport::Lens};

#[tokio::test]
async fn sdk_camera_optics_changes_real_projection_history_and_preserves_refused_state() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let original = editor.history.scene().clone();
    let matrix = editor.camera.projection_matrix(editor.aspect).unwrap();
    let optics = CameraOptics {
        focal_length_mm: 45.123456789012345,
        film_roll_radians: 0.35,
        film_offset_in: [0.125, -0.0625],
        ..Default::default()
    };
    for outcome in ["changed", "unchanged"] {
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_optics",
            json!({"scene_token":token,"camera":"persp","optics":optics}),
        )
        .await;
        assert_eq!(result.structured_content.unwrap()["outcome"], outcome);
        assert_eq!(
            editor.history.scene().camera_optics(SceneCamera::Persp),
            optics
        );
        assert_eq!(editor.camera.lens(), Lens::Optics(optics));
        assert_ne!(
            editor.camera.projection_matrix(editor.aspect).unwrap(),
            matrix
        );
    }
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
        inspected["inspection"]["camera"]["optics"][0]["optics"],
        json!(optics)
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "undo",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.history.scene(), &original);
    assert_eq!(
        editor.camera.lens(),
        Lens::Optics(CameraOptics::node_default())
    );
    call(
        &mut editor,
        &context,
        client.peer(),
        "redo",
        json!({"scene_token":token}),
    )
    .await;
    let baseline = editor.history.scene().clone();
    for arguments in [
        json!({"scene_token":token,"camera":"custom","optics":optics}),
        json!({"scene_token":token,"camera":"persp","optics":{},"extra":1}),
    ] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "set_camera_optics",
                arguments
            )
            .await
            .is_err()
        );
        assert_eq!(editor.history.scene(), &baseline);
    }
    let mut invalid = optics;
    invalid.overscan = 0.0;
    assert!(
        call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_optics",
            json!({"scene_token":token,"camera":"persp","optics":invalid})
        )
        .await
        .is_err()
    );
    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_optics",
        json!({"scene_token":Uuid::new_v4().to_string(),"camera":"persp","optics":optics}),
    )
    .await;
    assert_eq!(
        stale.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    editor.create(Primitive::Cube);
    editor.name = "unfinished draft".into();
    let baseline = editor.history.scene().clone();
    let refused = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_optics",
        json!({"scene_token":token,"camera":"persp","optics":optics}),
    )
    .await;
    assert_eq!(
        refused.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.history.scene(), &baseline);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn sdk_camera_optics_native_save_and_protocol_reopen_keep_exact_values() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    let directory = tempfile::tempdir().unwrap();
    let optics = CameraOptics {
        focal_length_mm: 52.123456789012345,
        film_roll_radians: f64::from_bits(0x3fe0000000000001),
        ..Default::default()
    };
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_optics",
        json!({"scene_token":token,"camera":"persp","optics":optics}),
    )
    .await;
    let destination = directory.path().join("camera.ma");
    let saved = call(
        &mut editor,
        &context,
        client.peer(),
        "save_scene_as",
        json!({"scene_token":token,"path":destination}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(saved["outcome"], "saved");
    let opened = call(
        &mut editor,
        &context,
        client.peer(),
        "open_scene",
        json!({"scene_token":token,"path":destination}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(opened["outcome"], "document_applied");
    assert_eq!(
        editor.history.scene().camera_optics(SceneCamera::Persp),
        optics
    );
    assert_eq!(editor.camera.lens(), Lens::Optics(optics));
    let view =
        azteccraft::view_scene::ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024)
            .unwrap();
    for instance in view.instances() {
        for vertex in instance.world_vertices() {
            let ndc = editor
                .camera
                .project(*vertex, editor.aspect)
                .unwrap()
                .unwrap();
            assert!(
                ndc.truncate().abs().max_element() <= 1.0001,
                "reopened geometry clipped: {ndc}"
            );
        }
    }

    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
