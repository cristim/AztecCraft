use super::*;
use azteccraft::viewport::{FilmBack, FilmFit, Lens};

fn film(editor: &Editor) -> FilmBack {
    match editor.camera.lens() {
        Lens::Film(film) => film,
        other => panic!("expected a film back, got {other:?}"),
    }
}

fn bits(film: &FilmBack) -> Vec<u64> {
    vec![
        film.focal_mm.to_bits(),
        film.aperture_in[0].to_bits(),
        film.aperture_in[1].to_bits(),
        film.film_fit as u64,
        film.film_fit_offset_in.to_bits(),
        film.film_offset_in[0].to_bits(),
        film.film_offset_in[1].to_bits(),
        film.lens_squeeze.to_bits(),
        film.pre_scale.to_bits(),
        film.post_scale.to_bits(),
        film.film_roll_degrees.to_bits(),
        film.overscan.to_bits(),
        u64::from(film.display_film_gate),
    ]
}

fn code(result: &rmcp::model::CallToolResult) -> String {
    result.structured_content.as_ref().unwrap()["error"]["code"]
        .as_str()
        .map_or_else(|| format!("no error: {result:?}"), str::to_owned)
}

/// Every field with a value no other field shares, so a swapped slot or a lost bit fails.
fn every_field(token: &str) -> Value {
    json!({
        "scene_token": token,
        "focal_length": 35.0_f64.next_down(),
        "horizontal_film_aperture": 1.25,
        "vertical_film_aperture": 0.8125,
        "film_fit": "horizontal",
        "film_fit_offset": 0.015625,
        "horizontal_film_offset": 0.03125,
        "vertical_film_offset": -0.0625,
        "lens_squeeze_ratio": 1.5,
        "overscan": 1.2,
        "pre_scale": 1.1,
        "post_scale": 0.9,
        "film_roll_value": 33.3,
        "display_film_gate": true,
    })
}

#[tokio::test]
async fn sdk_set_camera_film_back_sets_each_field_exactly_and_edits_only_the_named_ones() {
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

    let first = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        every_field(&text),
    )
    .await;
    assert_eq!(first.structured_content.unwrap()["outcome"], "changed");
    let expected = FilmBack {
        focal_mm: 35.0_f64.next_down(),
        aperture_in: [1.25, 0.8125],
        film_fit: FilmFit::Horizontal,
        film_fit_offset_in: 0.015625,
        film_offset_in: [0.03125, -0.0625],
        lens_squeeze: 1.5,
        pre_scale: 1.1,
        post_scale: 0.9,
        film_roll_degrees: 33.3,
        overscan: 1.2,
        display_film_gate: true,
    };
    assert_eq!(bits(&film(&editor)), bits(&expected));

    let repeat = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        every_field(&text),
    )
    .await;
    assert_eq!(repeat.structured_content.unwrap()["outcome"], "unchanged");

    // Only the named field moves; null counts as omitted.
    let partial = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        json!({"scene_token": text, "overscan": 1.75, "focal_length": null}),
    )
    .await;
    assert_eq!(partial.structured_content.unwrap()["outcome"], "changed");
    let after = film(&editor);
    assert_eq!(
        bits(&after),
        bits(&FilmBack {
            overscan: 1.75,
            ..expected
        })
    );

    // Unchanged is numeric: -0.0 over 0.0 writes nothing and the stored bits stay.
    for (field, value) in [("film_roll_value", 0.0), ("film_fit_offset", 0.0)] {
        call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_film_back",
            json!({"scene_token": text, field: value}),
        )
        .await;
    }
    let zero = bits(&film(&editor));
    let negative = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        json!({"scene_token": text, "film_roll_value": -0.0, "film_fit_offset": -0.0}),
    )
    .await;
    assert_eq!(negative.structured_content.unwrap()["outcome"], "unchanged");
    assert_eq!(bits(&film(&editor)), zero);

    assert_eq!(editor.scene_token, token);
    assert_eq!(editor.is_dirty(), dirty);
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_camera_film_back_refusals_name_the_field_and_leave_the_camera_bit_identical() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        every_field(&text),
    )
    .await;
    let camera = editor.camera.clone();
    let status = editor.status.clone();
    for (arguments, field) in [
        (json!({"focal_length": 0}), "focal_mm"),
        (json!({"focal_length": -1}), "focal_mm"),
        (json!({"focal_length": 1e-300}), "angle_of_view"),
        (json!({"focal_length": 1e300}), "angle_of_view"),
        (
            json!({"horizontal_film_aperture": 0}),
            "horizontal_aperture",
        ),
        (json!({"vertical_film_aperture": -0.5}), "vertical_aperture"),
        (json!({"lens_squeeze_ratio": 0}), "lens_squeeze"),
        (json!({"overscan": 0}), "overscan"),
        (json!({"overscan": 1e4}), "overscan"),
        (json!({"pre_scale": 1e7}), "pre_scale"),
        (json!({"post_scale": 1e-7}), "post_scale"),
        (json!({"film_fit_offset": 1e300}), "film_fit_offset"),
        (
            json!({"horizontal_film_offset": 1e300}),
            "horizontal_film_offset",
        ),
        (
            json!({"vertical_film_offset": -1e300}),
            "vertical_film_offset",
        ),
        // One bad field refuses the whole edit: the valid focal length must not land.
        (
            json!({"focal_length": 80, "film_roll_value": 1e308, "pre_scale": 1e300}),
            "pre_scale",
        ),
        (json!({"focal_length": 80, "overscan": 0}), "overscan"),
        (json!({}), "no_fields"),
        (json!({"focal_length": null}), "no_fields"),
    ] {
        let mut arguments = arguments;
        arguments["scene_token"] = json!(text);
        let result = call(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_film_back",
            arguments.clone(),
        )
        .await;
        assert_eq!(result.is_error, Some(true), "{arguments}");
        assert_eq!(code(&result), "camera_error", "{arguments}");
        let message = result.structured_content.unwrap()["error"]["message"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(message.contains(field), "{arguments}: {message}");
        assert!(editor.camera.same_capture(&camera), "{arguments}");
        assert_eq!(editor.status, status);
    }
    // Malformed arguments are protocol errors, not camera errors.
    for arguments in [
        json!({"scene_token": text, "film_fit": "Fill"}),
        json!({"scene_token": text, "film_fit": "back"}),
        json!({"scene_token": text, "focal_length": "NaN"}),
        json!({"scene_token": text, "focal_length": [35]}),
        json!({"scene_token": text, "display_film_gate": 1}),
        json!({"scene_token": text, "focal_mm": 50}),
        json!({"focal_length": 50}),
    ] {
        let rejected = call_response(
            &mut editor,
            &context,
            client.peer(),
            "set_camera_film_back",
            arguments.clone(),
        )
        .await;
        let Err(rmcp::ServiceError::McpError(error)) = rejected else {
            panic!("expected a JSON-RPC error for {arguments}");
        };
        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(editor.camera.same_capture(&camera), "{arguments}");
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_camera_film_back_obeys_the_token_draft_modal_and_manipulation_guards() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let text = editor.scene_token.to_string();
    let edit = json!({"scene_token": text, "focal_length": 85});
    let camera = editor.camera.clone();
    let scene = editor.history.scene().clone();

    let stale = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        json!({"scene_token": Uuid::new_v4().to_string(), "focal_length": 85}),
    )
    .await;
    assert_eq!(code(&stale), "stale_scene", "stale");
    assert!(editor.camera.same_capture(&camera));

    editor.name = "human_draft".into();
    let draft = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        edit.clone(),
    )
    .await;
    assert_eq!(code(&draft), "busy_editing", "draft");
    assert_eq!(editor.name, "human_draft");
    assert!(editor.camera.same_capture(&camera));
    editor.sync_fields();

    let node = editor.selected().unwrap().to_decimal();
    let path: Vec<_> = editor
        .selected_path()
        .iter()
        .map(|id| id.to_decimal())
        .collect();
    let begin = call(
        &mut editor,
        &context,
        client.peer(),
        "begin_manipulation",
        json!({"scene_token": text, "node_id": node, "instance_path": path, "kind": "move", "axis": "x"}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(begin["outcome"], "manipulation_started", "{begin}");
    let held = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        edit.clone(),
    )
    .await;
    assert_eq!(code(&held), "busy_manipulation", "held");
    assert!(editor.camera.same_capture(&camera));
    call(
        &mut editor,
        &context,
        client.peer(),
        "finish_manipulation",
        json!({"scene_token": text, "manipulation_id": begin["manipulation_id"], "finish": "cancel"}),
    )
    .await;
    assert_eq!(editor.history.scene(), &scene);

    let ok = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        edit,
    )
    .await;
    assert_eq!(ok.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(film(&editor).focal_mm, 85.0);

    let camera = editor.camera.clone();
    editor.request_close();
    let modal = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        json!({"scene_token": text, "focal_length": 100}),
    )
    .await;
    assert_eq!(code(&modal), "busy_editing", "modal");
    assert!(editor.camera.same_capture(&camera));
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

#[tokio::test]
async fn sdk_set_camera_film_back_is_accepted_in_an_orthographic_view_without_changing_it() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let text = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_viewport_view",
        json!({"scene_token": text, "view": "top"}),
    )
    .await;
    let set = call(
        &mut editor,
        &context,
        client.peer(),
        "set_camera_film_back",
        json!({"scene_token": text, "focal_length": 50, "overscan": 1.5}),
    )
    .await;
    assert_eq!(set.structured_content.unwrap()["outcome"], "changed");
    assert_eq!(film(&editor).focal_mm, 50.0);
    let camera = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_scene",
        json!({}),
    )
    .await
    .structured_content
    .unwrap()["inspection"]["camera"]
        .clone();
    assert_eq!(camera["projection"], "orthographic");
    assert_eq!(camera["view"], "top");
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
