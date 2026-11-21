use super::*;

#[tokio::test]
async fn sdk_camera_navigation_inspects_real_view_without_authoring_changes() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.history_step(false);
    let before = editor.history.scene().clone();
    let token = editor.scene_token;
    let selected = editor.selected();
    let dirty = editor.is_dirty();
    let mut eye = editor.camera.eye();
    let mut target = editor.camera.target();
    let mut distance = editor.camera.distance();
    for (tool, fields) in [
        ("orbit_camera", json!({"radians":[0.2,-0.1]})),
        ("pan_camera", json!({"world_units":[1.0,-2.0]})),
        ("dolly_camera", json!({"factor":0.5})),
        ("orbit_camera", json!({"radians":[0.0,0.0]})),
        ("pan_camera", json!({"world_units":[0.0,0.0]})),
        ("dolly_camera", json!({"factor":1.0})),
    ] {
        let noop = fields["radians"] == json!([0.0, 0.0])
            || fields["world_units"] == json!([0.0, 0.0])
            || fields["factor"] == json!(1.0);
        if !noop {
            match tool {
                "orbit_camera" => {
                    let yaw = 0.8_f32;
                    let pitch = 0.3_f32;
                    eye = target
                        + distance
                            * Vec3::new(
                                yaw.sin() * pitch.cos(),
                                pitch.sin(),
                                yaw.cos() * pitch.cos(),
                            );
                }
                "pan_camera" => {
                    let back = (eye - target).normalize();
                    let right = Vec3::Y.cross(back).normalize();
                    let up = back.cross(right);
                    let translation = right - 2.0 * up;
                    target += translation;
                    eye += translation;
                }
                _ => {
                    distance *= 0.5;
                    eye = target + (eye - target) * 0.5;
                }
            }
        }
        let mut arguments = fields;
        arguments["scene_token"] = json!(token.to_string());
        let result = call(&mut editor, &context, client.peer(), tool, arguments).await;
        assert_eq!(
            result.structured_content.unwrap()["outcome"],
            if noop { "unchanged" } else { "changed" }
        );
        assert!((editor.camera.eye() - eye).length() < 0.00001);
        assert!((editor.camera.target() - target).length() < 0.00001);
        assert_eq!(editor.camera.distance(), distance);
        let inspected = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await;
        let camera = &inspected.structured_content.unwrap()["inspection"]["camera"];
        for (field, expected) in [
            ("target", editor.camera.target()),
            ("eye", editor.camera.eye()),
        ] {
            let actual: [f32; 3] = serde_json::from_value(camera[field].clone()).unwrap();
            assert_eq!(actual, expected.to_array());
        }
        assert_eq!(camera["distance"].as_f64().unwrap() as f32, distance);
        assert_eq!(
            camera["center_of_interest_cm"].as_f64().unwrap() as f32,
            distance
        );
        assert_eq!(camera["focal_length_mm"], 35.0);
        for (field, expected) in [
            ("yaw_degrees", f64::from(editor.camera.yaw()).to_degrees()),
            (
                "pitch_degrees",
                f64::from(editor.camera.pitch()).to_degrees(),
            ),
            (
                "fov_y_degrees",
                f64::from(editor.camera.fov_y_for(editor.aspect)).to_degrees(),
            ),
            ("near_clip", f64::from(editor.camera.near())),
            ("far_clip", f64::from(editor.camera.far())),
        ] {
            assert_eq!(camera[field].as_f64().unwrap(), expected, "{field}");
        }
        // The reported pose reproduces the eye, so a client can verify a navigation result.
        let (yaw, pitch) = (
            camera["yaw_degrees"].as_f64().unwrap().to_radians(),
            camera["pitch_degrees"].as_f64().unwrap().to_radians(),
        );
        let offset = Vec3::new(
            (yaw.sin() * pitch.cos()) as f32,
            pitch.sin() as f32,
            (yaw.cos() * pitch.cos()) as f32,
        ) * distance;
        assert!((target + offset - eye).length() < 0.001);
        assert_eq!(editor.history.scene(), &before);
        assert!(!editor.history.can_undo());
        assert!(editor.history.can_redo());
        assert_eq!(editor.selected(), selected);
        assert_eq!(editor.scene_token, token);
        assert_eq!(editor.is_dirty(), dirty);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_camera_rejects_bad_parameters_and_invalid_ranges_without_mutation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let camera = editor.camera.clone();
    let token = editor.scene_token.to_string();
    for (tool, fields) in [
        ("orbit_camera", json!({"radians":[1e100,0]})),
        ("pan_camera", json!({"world_units":[1e100,0]})),
        ("pan_camera", json!({"world_units":[3e38,3e38]})),
        ("dolly_camera", json!({"factor":0})),
        ("dolly_camera", json!({"factor":-1})),
        ("dolly_camera", json!({"factor":1e100})),
        ("dolly_camera", json!({"factor":3e38})),
        ("dolly_camera", json!({"factor":1e-45})),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        let result = call(&mut editor, &context, client.peer(), tool, args).await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            "camera_error"
        );
        assert_eq!(editor.camera, camera);
        assert!(!editor.history.can_undo());
        assert!(!editor.is_dirty());
    }
    for (tool, fields) in [
        ("orbit_camera", json!({"radians":[0]})),
        ("orbit_camera", json!({"radians":[0,0,0]})),
        ("orbit_camera", json!({"radians":["0",0]})),
        ("pan_camera", json!({"world_units":null})),
        ("pan_camera", json!({"world_units":[0,0],"extra":true})),
        ("dolly_camera", json!({"factor":"1"})),
        ("dolly_camera", json!({})),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        assert!(
            call_response(&mut editor, &context, client.peer(), tool, args)
                .await
                .is_err()
        );
        assert_eq!(editor.camera, camera);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "requires a real headless GPU adapter"]
async fn headless_gpu_sdk_camera_selection_and_grid_changes_enter_the_next_fresh_callback() {
    use crate::viewport_renderer::tests::{creation_context, recorded_submission};
    use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
    let context = egui::Context::default();
    let (mut temporary, client, server) = start(&context).await;
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.mcp = temporary.mcp.take();
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    frame(&mut editor, &context, vec![]);
    let id = editor.selected().unwrap().to_decimal();
    let path = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect::<Vec<_>>();
    let scene = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    let view = ViewScene::from_native(editor.history.scene(), 256 * 1024 * 1024).unwrap();
    let instance = view
        .instances()
        .iter()
        .find(|instance| instance.transform().to_decimal() == id)
        .unwrap();
    let face_target = json!({
        "node_id": id,
        "shape_id": instance.shape().to_decimal(),
        "instance_path": instance.path().iter().map(|id| id.to_decimal()).collect::<Vec<_>>(),
        "face_index": 1,
    });
    for (tool, arguments) in [
        ("dolly_camera", json!({"scene_token":token,"factor":0.75})),
        ("select_node", json!({"scene_token":token,"node_id":null})),
        (
            "select_node",
            json!({"scene_token":token,"node_id":id,"instance_path":path}),
        ),
        ("frame_selected", json!({"scene_token":token})),
        (
            "set_grid_visibility",
            json!({"scene_token":token,"visible":false}),
        ),
        (
            "set_selection_mode",
            json!({"scene_token":token,"mode":"face"}),
        ),
        (
            "select_face",
            json!({"scene_token":token,"target":face_target}),
        ),
    ] {
        let before = recorded_submission(editor.renderer.as_ref().unwrap()).vertices;
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(tool).with_arguments(arguments.as_object().unwrap().clone()),
        ));
        let handle = client
            .peer()
            .send_cancellable_request(request, Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        frame(&mut editor, &context, vec![]);
        assert_eq!(
            recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
            before,
            "remote changes cannot alter the already prepared callback"
        );
        let response = tokio::time::timeout(protocol_call_deadline(), handle.await_response())
            .await
            .unwrap()
            .unwrap();
        let ServerResult::CallToolResult(response) = response else {
            panic!("tool response required")
        };
        assert_ne!(response.is_error, Some(true));
        frame(&mut editor, &context, vec![]);
        assert_ne!(
            recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
            before,
            "the next actual App frame must use current authoritative state"
        );
        let view = azteccraft::view_scene::ViewScene::from_native(
            editor.history.scene(),
            256 * 1024 * 1024,
        )
        .unwrap();
        assert_eq!(
            editor
                .camera
                .pick_view(&view, glam::Vec2::ZERO, editor.aspect)
                .unwrap()
                .unwrap()
                .transform
                .to_decimal(),
            id
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.history.can_undo(), undo);
    }
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    let inspection = &result["inspection"];
    assert_eq!(inspection["viewport_backend"], "wgpu_geometry_preview");
    assert_eq!(inspection["viewport_error"], Value::Null);
    assert_eq!(inspection["selected_instance_path"], json!(path));
    assert_eq!(inspection["depth_complete"], true);
    assert_eq!(inspection["clipping_complete"], true);
    assert_eq!(inspection["grid_visible"], false);
    assert_eq!(inspection["selection_mode"], "face");
    assert_eq!(inspection["remembered_face"], face_target);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
#[ignore = "requires a real headless GPU adapter"]
async fn headless_gpu_sdk_hidden_inspection_reads_live_terminal_health_without_ui_paint() {
    use crate::viewport_renderer::tests::{creation_context, recorded_submission};
    use rmcp::model::{CallToolRequest, ClientRequest, ServerResult};
    let context = egui::Context::default();
    let (mut temporary, client, server) = start(&context).await;
    let creation = creation_context(context.clone());
    let mut editor = Editor::new(&creation).unwrap();
    editor.mcp = temporary.mcp.take();
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "create_primitive",
        json!({"scene_token":token,"primitive":"cube"}),
    )
    .await;
    frame(&mut editor, &context, vec![]);
    let before = recorded_submission(editor.renderer.as_ref().unwrap()).vertices;
    let scene = editor.history.scene().clone();
    let undo = editor.history.can_undo();
    let state = creation.wgpu_render_state.as_ref().unwrap();
    let _invalid = state
        .device
        .create_shader_module(eframe::wgpu::ShaderModuleDescriptor {
            label: Some("private-hidden-driver-marker"),
            source: eframe::wgpu::ShaderSource::Wgsl("invalid shader syntax".into()),
        });
    state
        .device
        .poll(eframe::wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(10)),
        })
        .unwrap();
    for (tool, arguments) in [
        ("inspect_scene", json!({})),
        ("dolly_camera", json!({"scene_token":token,"factor":1.0})),
        ("inspect_scene", json!({})),
    ] {
        let request = ClientRequest::CallToolRequest(CallToolRequest::new(
            CallToolRequestParams::new(tool).with_arguments(arguments.as_object().unwrap().clone()),
        ));
        let handle = client
            .peer()
            .send_cancellable_request(request, Default::default())
            .await
            .unwrap();
        wait_for_queue(&editor).await;
        hidden_frame(&mut editor, &context, vec![]);
        let response = tokio::time::timeout(protocol_call_deadline(), handle.await_response())
            .await
            .unwrap()
            .unwrap();
        let ServerResult::CallToolResult(response) = response else {
            panic!("tool response required")
        };
        assert_ne!(response.is_error, Some(true));
        if tool == "inspect_scene" {
            let content = response.structured_content.unwrap();
            let inspection = &content["inspection"];
            assert_eq!(inspection["viewport_backend"], "wgpu_geometry_preview");
            assert_eq!(inspection["viewport_error"], "backend_failed");
            assert_eq!(inspection["depth_complete"], false);
            assert_eq!(inspection["clipping_complete"], false);
            assert!(!content.to_string().contains("private-hidden-driver-marker"));
        }
        assert_eq!(
            recorded_submission(editor.renderer.as_ref().unwrap()).vertices,
            before
        );
        assert_eq!(editor.history.scene(), &scene);
        assert_eq!(editor.history.can_undo(), undo);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

pub(super) async fn inspected_camera(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> Value {
    call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap()["inspection"]["camera"]
        .clone()
}

fn pose_arguments(token: &str, camera: &Value) -> Value {
    json!({
        "scene_token": token,
        "target": camera["target"],
        "distance": camera["distance"],
        "yaw_degrees": camera["yaw_degrees"],
        "pitch_degrees": camera["pitch_degrees"],
    })
}

#[tokio::test]
async fn sdk_set_camera_pose_restores_an_inspected_pose_exactly() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.history_step(false);
    let (token, dirty, scene) = (
        editor.scene_token,
        editor.is_dirty(),
        editor.history.scene().clone(),
    );
    let text = token.to_string();
    editor.camera.set_pose(Vec3::ZERO, 8.0, 0.0, 0.4).unwrap();
    for (tool, fields) in [
        // From yaw 0 a tiny negative step must not wrap to a full turn.
        ("orbit_camera", json!({"radians":[-1e-9,0.0]})),
        ("orbit_camera", json!({"radians":[0.7,-0.2]})),
        ("pan_camera", json!({"world_units":[1.3,-2.1]})),
        ("dolly_camera", json!({"factor":0.37})),
        ("orbit_camera", json!({"radians":[9.0,10.0]})),
    ] {
        let mut arguments = fields;
        arguments["scene_token"] = json!(text);
        call(&mut editor, &context, client.peer(), tool, arguments).await;
        let saved = editor.camera.clone();
        let mut read = inspected_camera(&mut editor, &context, &client).await;
        let same = call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_pose",
            pose_arguments(&text, &read),
        )
        .await;
        assert_eq!(same.structured_content.unwrap()["outcome"], "unchanged");
        assert!(editor.camera.same_capture(&saved), "{tool}");
        editor.camera.orbit(glam::Vec2::new(1.0, 0.3)).unwrap();
        editor.camera.dolly(3.0).unwrap();
        let moved = call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_pose",
            pose_arguments(&text, &read),
        )
        .await;
        assert_eq!(moved.structured_content.unwrap()["outcome"], "changed");
        assert!(editor.camera.same_capture(&saved), "{tool}");
        // Restoring a changed camera adds a view-history entry, independent of pose equality.
        read["can_undo_view"] = json!(true);
        assert_eq!(inspected_camera(&mut editor, &context, &client).await, read);
    }
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_camera_pose_wraps_yaw_and_rejects_bad_input_without_mutation() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    let valid = json!({
        "scene_token": token,
        "target": [1, 2, 3],
        "distance": 4,
        "yaw_degrees": -90,
        "pitch_degrees": 10,
    });
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_pose",
        valid.clone(),
    )
    .await;
    assert!((editor.camera.yaw().to_degrees() - 270.0).abs() < 1e-3);
    let camera = editor.camera.clone();
    let status = editor.status.clone();
    let with = |field: &str, value: Value| {
        let mut args = valid.clone();
        args[field] = value;
        args
    };
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("distance");
    let mut extra = valid.clone();
    extra["fov_y_degrees"] = json!(30);
    for (args, text) in [
        (with("pitch_degrees", json!(90)), "pitch_degrees"),
        (with("pitch_degrees", json!(-89.95)), "pitch_degrees"),
        (with("distance", json!(0)), "distance"),
        (with("distance", json!(-1)), "distance"),
        (with("distance", json!(1e300)), "distance"),
        (with("target", json!([1e300, 0, 0])), "target"),
        (with("yaw_degrees", json!(1e300)), "yaw_degrees"),
        (with("target", json!([0, 0])), ""),
        (with("target", json!(["NaN", 0, 0])), ""),
        (with("yaw_degrees", json!("NaN")), ""),
        (with("distance", json!(null)), ""),
        (missing, ""),
        (extra, ""),
    ] {
        let error = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_pose",
            args,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains(text), "{error}");
        assert_eq!(editor.camera, camera);
        assert_eq!(editor.status, status);
    }
    let stale = with("scene_token", json!(uuid::Uuid::new_v4().to_string()));
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_pose",
        stale,
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "stale_scene"
    );
    assert_eq!(editor.camera, camera);
    assert_eq!(editor.status, status);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_camera_pose_converts_degrees_in_double_precision() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let token = editor.scene_token.to_string();
    for step in 1..400 {
        let degrees = f64::from(step) * 0.7311;
        let pitch_degrees = degrees % 89.0;
        let arguments = json!({
            "scene_token": token,
            "target": [0, 0, 0],
            "distance": 3,
            "yaw_degrees": degrees,
            "pitch_degrees": pitch_degrees,
        });
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_pose",
            arguments,
        )
        .await;
        assert_eq!(
            editor.camera.yaw().to_bits(),
            (degrees.to_radians() as f32)
                .rem_euclid(std::f32::consts::TAU)
                .to_bits()
        );
        assert_eq!(
            editor.camera.pitch().to_bits(),
            (pitch_degrees.to_radians() as f32).to_bits()
        );
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_camera_focal_length_is_unknown_for_a_field_of_view_only_lens() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.camera = Camera::new(Vec3::ZERO, 0.0, 0.0, 12.0, 0.7, 0.1, 100.0).unwrap();
    let token = editor.scene_token;
    let before = editor.history.scene().clone();
    let camera = inspected_camera(&mut editor, &context, &client).await;
    assert!(camera["focal_length_mm"].is_null());
    assert!(camera["film_back"].is_null());
    assert_eq!(camera["center_of_interest_cm"], 12.0);
    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.history.scene(), &before);
    assert!(!editor.is_dirty());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_box_dolly_uses_the_live_aspect_and_preserves_authoring() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    for _ in 0..5 {
        frame(&mut editor, &context, vec![]);
    }
    let token = editor.scene_token.to_string();
    let scene = editor.history.scene().clone();
    let mut expected = editor.camera.clone();
    expected
        .box_dolly(
            azteccraft::viewport::NdcRect::new(glam::Vec2::ZERO, glam::Vec2::ONE).unwrap(),
            editor.aspect,
        )
        .unwrap();
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "box_dolly_camera",
        json!({"scene_token":token,"min":[0,0],"max":[1,1]}),
    )
    .await;
    assert_eq!(result.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(editor.camera, expected);
    assert_eq!(editor.history.scene(), &scene);
    assert!(!editor.is_dirty());
    assert!(!editor.history.can_undo());
    let baseline = editor.camera.clone();
    for (fields, code) in [
        (json!({"min":[0,0],"max":[0,1]}), "camera_error"),
        (json!({"min":[0,0],"max":[0,0]}), "camera_error"),
        (json!({"min":[-1,-1],"max":[1,1]}), ""),
        (json!({"min":[0,0],"max":[1.00000000001,1]}), "protocol"),
        (json!({"min":[0,0],"max":[1e100,1]}), "protocol"),
        (json!({"min":[0],"max":[1,1]}), "protocol"),
        (json!({"min":[0,0],"max":[1,1],"extra":true}), "protocol"),
    ] {
        let mut args = fields;
        args["scene_token"] = json!(token);
        let result = call_response(
            &mut editor,
            &context,
            client.peer(),
            "box_dolly_camera",
            args,
        )
        .await;
        if code == "protocol" {
            assert!(result.is_err());
        } else if code.is_empty() {
            assert_eq!(
                result.unwrap().structured_content.unwrap()["outcome"],
                "unchanged"
            );
        } else {
            assert_eq!(
                result.unwrap().structured_content.unwrap()["error"]["code"],
                code
            );
        }
        assert_eq!(editor.camera, baseline);
    }
    let pos = egui::pos2(600.0, 400.0);
    frame(
        &mut editor,
        &context,
        vec![
            egui::Event::ModifiersChanged(egui::Modifiers::ALT),
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::ALT,
            },
        ],
    );
    frame(&mut editor, &context, vec![]);
    assert!(editor.native_camera_drag);
    let read = inspected_camera(&mut editor, &context, &client).await;
    assert_eq!(read["native_gesture_active"], true);
    let result = call(
        &mut editor,
        &context,
        client.peer(),
        "box_dolly_camera",
        json!({"scene_token":token,"min":[0,0],"max":[1,1]}),
    )
    .await;
    assert_eq!(
        result.structured_content.unwrap()["error"]["code"],
        "busy_editing"
    );
    assert_eq!(editor.camera, baseline);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_box_dolly_respects_scene_draft_modal_close_and_manipulation_guards() {
    for guard in ["stale", "draft", "modal", "close", "manipulation"] {
        let ctx = egui::Context::default();
        let (mut editor, client, server) = start(&ctx).await;
        if guard != "close" {
            editor.create(Primitive::Cube);
        }
        let token = editor.scene_token.to_string();
        let expected = match guard {
            "draft" => {
                editor.channels[0] = "7".into();
                "busy_editing"
            }
            "modal" => {
                editor.new_document().unwrap();
                "busy_editing"
            }
            "close" => {
                editor.close_document().unwrap();
                "busy_editing"
            }
            "manipulation" => {
                let node = editor.selected().unwrap().to_decimal();
                let path: Vec<_> = editor
                    .selected_path()
                    .iter()
                    .map(|id| id.to_decimal())
                    .collect();
                let result=call(&mut editor,&ctx,client.peer(),"begin_manipulation",json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"})).await;
                assert_eq!(
                    result.structured_content.unwrap()["outcome"],
                    "manipulation_started"
                );
                "busy_manipulation"
            }
            _ => "stale_scene",
        };
        let before = editor.camera.clone();
        let scene = editor.history.scene().clone();
        let requested_token = if guard == "stale" {
            Uuid::new_v4().to_string()
        } else {
            token
        };
        let result = call(
            &mut editor,
            &ctx,
            client.peer(),
            "box_dolly_camera",
            json!({"scene_token":requested_token,"min":[-0.5,-0.5],"max":[0.5,0.5]}),
        )
        .await;
        assert_eq!(
            result.structured_content.unwrap()["error"]["code"],
            expected,
            "{guard}"
        );
        assert_eq!(editor.camera, before);
        assert_eq!(editor.history.scene(), &scene);
        if guard == "draft" {
            assert_eq!(editor.channels[0], "7");
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn sdk_view_history_round_trips_camera_and_exposes_availability() {
    let ctx = egui::Context::default();
    let (mut editor, client, server) = start(&ctx).await;
    let token = editor.scene_token.to_string();
    let initial = editor.camera.clone();
    for (tool, args) in [
        ("undo_view_change", json!({"scene_token":token})),
        ("redo_view_change", json!({"scene_token":token})),
    ] {
        let result = call(&mut editor, &ctx, client.peer(), tool, args).await;
        assert_eq!(result.structured_content.unwrap()["outcome"], "unchanged");
    }
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "dolly_camera",
        json!({"scene_token":token,"factor":0.5}),
    )
    .await;
    let changed = editor.camera.clone();
    assert_eq!(
        inspected_camera(&mut editor, &ctx, &client).await["can_undo_view"],
        true
    );
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "undo_view_change",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.camera, initial);
    assert_eq!(
        inspected_camera(&mut editor, &ctx, &client).await["can_redo_view"],
        true
    );
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "redo_view_change",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.camera, changed);
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token":token,"view":"top"}),
    )
    .await;
    let before = editor.camera.clone();
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "default_camera_view",
        json!({"scene_token":token}),
    )
    .await;
    call(
        &mut editor,
        &ctx,
        client.peer(),
        "undo_view_change",
        json!({"scene_token":token}),
    )
    .await;
    assert_eq!(editor.camera, before);
    assert!(!editor.is_dirty());
    assert!(!editor.history.can_undo());
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_view_history_has_strict_schemas_and_editor_guards() {
    for tool in [
        "undo_view_change",
        "redo_view_change",
        "default_camera_view",
    ] {
        for guard in ["stale", "draft", "modal", "close", "manipulation"] {
            let ctx = egui::Context::default();
            let (mut editor, client, server) = start(&ctx).await;
            if guard != "close" {
                editor.create(Primitive::Cube);
            }
            let token = editor.scene_token.to_string();
            editor
                .apply_operation(Operation::Dolly { factor: 0.5 })
                .unwrap();
            let expected = match guard {
                "draft" => {
                    editor.channels[0] = "7".into();
                    "busy_editing"
                }
                "modal" => {
                    editor.new_document().unwrap();
                    "busy_editing"
                }
                "close" => {
                    editor.close_document().unwrap();
                    "busy_editing"
                }
                "manipulation" => {
                    let node = editor.selected().unwrap().to_decimal();
                    let path: Vec<_> = editor
                        .selected_path()
                        .iter()
                        .map(|id| id.to_decimal())
                        .collect();
                    let result=call(&mut editor,&ctx,client.peer(),"begin_manipulation",json!({"scene_token":token,"node_id":node,"instance_path":path,"kind":"move","axis":"x"})).await;
                    assert_eq!(
                        result.structured_content.unwrap()["outcome"],
                        "manipulation_started"
                    );
                    "busy_manipulation"
                }
                _ => "stale_scene",
            };
            let before = editor.camera.clone();
            let lengths = (
                editor.view_history.undo.len(),
                editor.view_history.redo.len(),
            );
            let requested = if guard == "stale" {
                Uuid::new_v4().to_string()
            } else {
                token.clone()
            };
            let result = call(
                &mut editor,
                &ctx,
                client.peer(),
                tool,
                json!({"scene_token":requested}),
            )
            .await;
            assert_eq!(
                result.structured_content.unwrap()["error"]["code"],
                expected,
                "{tool} {guard}"
            );
            assert_eq!(editor.camera, before);
            assert_eq!(
                (
                    editor.view_history.undo.len(),
                    editor.view_history.redo.len()
                ),
                lengths
            );
            assert!(
                call_response(
                    &mut editor,
                    &ctx,
                    client.peer(),
                    tool,
                    json!({"scene_token":token,"extra":true})
                )
                .await
                .is_err()
            );
            assert!(
                call_response(&mut editor, &ctx, client.peer(), tool, json!({}))
                    .await
                    .is_err()
            );
            client.cancel().await.unwrap();
            drop(editor);
            server.await.unwrap();
        }
    }
}
