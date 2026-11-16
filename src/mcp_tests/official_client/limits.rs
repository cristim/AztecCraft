use super::*;

#[tokio::test]
async fn sdk_inspect_limits_serves_the_core_registry_without_a_token_or_state_change() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.create(Primitive::Cube);
    editor.name = "temporary".into();
    assert!(editor.commit_pending());
    frame(&mut editor, &context, vec![]);
    let baseline = serde_json::to_value(editor.inspect(false).unwrap()).unwrap();
    let scene = editor.history.scene().clone();

    let reply = call(
        &mut editor,
        &context,
        client.peer(),
        "inspect_limits",
        json!({}),
    )
    .await
    .structured_content
    .unwrap();
    assert_eq!(reply["outcome"], "scene_limits");
    let expected = serde_json::to_value(azteccraft::scene_limits::scene_limits()).unwrap();
    assert_eq!(reply["limits"], expected);
    let rows = reply["limits"].as_array().unwrap();
    assert!(rows.iter().any(|row| {
        row["name"] == "instance_paths" && row["value"] == 4096 && row["unit"] == "count"
    }));
    assert!(
        rows.iter()
            .any(|row| row["name"] == "view_bytes" && row["unit"] == "bytes")
    );

    for arguments in [json!({"scene_token": "x"}), json!({"extra": 1})] {
        assert!(
            call_response(
                &mut editor,
                &context,
                client.peer(),
                "inspect_limits",
                arguments
            )
            .await
            .is_err()
        );
    }
    assert_eq!(
        serde_json::to_value(editor.inspect(false).unwrap()).unwrap(),
        baseline
    );
    assert_eq!(editor.history.scene(), &scene);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

async fn usage(editor: &mut Editor, context: &egui::Context, peer: &Peer<RoleClient>) -> Value {
    let reply = call(editor, context, peer, "inspect_limits", json!({}))
        .await
        .structured_content
        .unwrap();
    reply["usage"].clone()
}

fn cap<'a>(usage: &'a Value, name: &str) -> &'a Value {
    usage["caps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap()
}

#[tokio::test]
async fn sdk_inspect_limits_usage_counts_an_empty_scene_primitives_joints_and_keys() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let empty = usage(&mut editor, &context, client.peer()).await;
    assert_eq!(empty["statistics"]["nodes"], 0);
    let view = cap(&empty, "view_bytes");
    assert_eq!(view["estimate"], "measured");
    assert_eq!(view["limit"], 256 * 1024 * 1024);
    assert_eq!(
        view["used"].as_u64().unwrap() + view["headroom"].as_u64().unwrap(),
        256 * 1024 * 1024
    );

    editor.create(Primitive::Cube);
    editor.create(Primitive::Cube);
    let cubes = usage(&mut editor, &context, client.peer()).await;
    let stats = &cubes["statistics"];
    assert_eq!(
        (&stats["nodes"], &stats["shapes"], &stats["vertices"]),
        (&json!(2), &json!(2), &json!(16))
    );
    assert_eq!(
        (&stats["edges"], &stats["faces"], &stats["triangles"]),
        (&json!(24), &json!(12), &json!(24))
    );
    assert_eq!(cap(&cubes, "instance_paths")["used"], 2);
    assert!(cap(&cubes, "view_bytes")["used"].as_u64() > view["used"].as_u64());

    for position in [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]] {
        let arguments =
            json!({"scene_token": editor.scene_token.to_string(), "position": position});
        let created = call(
            &mut editor,
            &context,
            client.peer(),
            "create_joint",
            arguments,
        )
        .await;
        assert_eq!(created.structured_content.unwrap()["outcome"], "created");
    }
    editor.create(Primitive::Cube);
    let token = editor.scene_token.to_string();
    let keyed = call(
        &mut editor,
        &context,
        client.peer(),
        "set_keyframe",
        json!({"scene_token": token}),
    )
    .await;
    assert_eq!(keyed.structured_content.unwrap()["outcome"], "changed");
    let rigged = usage(&mut editor, &context, client.peer()).await;
    assert_eq!(rigged["statistics"]["joints"], 2);
    assert_eq!(
        rigged["statistics"]["keys"].as_u64().unwrap(),
        editor.history.scene().session_key_count() as u64
    );
    assert!(rigged["statistics"]["keys"].as_u64().unwrap() > 0);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// Headroom reaches zero at the depth cap (a cube under 255 nested groups is 256 deep); the path
/// cap is covered in core, where 4096 cubes do not slow every editor frame.
#[tokio::test]
async fn sdk_inspect_limits_usage_headroom_reaches_zero_at_the_depth_cap_and_never_goes_negative() {
    use azteccraft::native_scene::{GroupOptions, GroupPivot, GroupUnder, MAX_TRANSFORM_DEPTH};
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let mut scene = azteccraft::native_scene::NativeScene::default();
    let mut top = scene.create(Primitive::Cube, "leaf").unwrap();
    let options = GroupOptions {
        under: GroupUnder::World,
        pivot: GroupPivot::Origin,
        preserve_position: true,
    };
    for _ in 1..MAX_TRANSFORM_DEPTH {
        top = scene.group(&[top], options).unwrap();
    }
    editor.history = crate::app::History::new(scene);
    editor.sync_fields();
    let full = usage(&mut editor, &context, client.peer()).await;
    let depth = cap(&full, "instance_depth");
    assert_eq!(depth["used"], MAX_TRANSFORM_DEPTH);
    assert_eq!(depth["headroom"], 0);
    assert_eq!(full["statistics"]["nodes"], MAX_TRANSFORM_DEPTH);
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}

/// The statistics match what `inspect_scene` reports for the same scene: two cubes, and a
/// 4-control-vertex curve imported from Maya ASCII.
#[tokio::test]
async fn sdk_inspect_limits_usage_agrees_with_inspect_scene_for_shapes_and_curves() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    let source = "//Maya ASCII 2016 scene\nrequires maya \"2016\"; currentUnit -l centimeter -a degree -t film;\n\
        createNode transform -n \"c1\"; createNode nurbsCurve -n \"c1Shape\" -p \"c1\";\n\
        setAttr \".cc\" -type \"nurbsCurve\" 3 1 0 no 3 6 0 0 0 1 1 1 4 0 0 0 1 3 0 2 -3 0 3 0 0;";
    let curve_scene =
        azteccraft::native_scene::ascii::decode(source.as_bytes().to_vec().into()).unwrap();
    let mut cube_scene = azteccraft::native_scene::NativeScene::default();
    cube_scene.create(Primitive::Cube, "a").unwrap();
    cube_scene.create(Primitive::Cube, "b").unwrap();
    for (scene, curve_count, cv_count) in [(curve_scene, 1, 4), (cube_scene, 0, 0)] {
        editor.history = crate::app::History::new(scene);
        editor.sync_fields();
        let reply = usage(&mut editor, &context, client.peer()).await;
        let inspection = call(
            &mut editor,
            &context,
            client.peer(),
            "inspect_scene",
            json!({}),
        )
        .await
        .structured_content
        .unwrap()["inspection"]
            .clone();
        let stats = &reply["statistics"];
        assert_eq!(
            stats["nodes"],
            inspection["nodes"].as_array().unwrap().len()
        );
        assert_eq!(
            stats["shapes"],
            inspection["shapes"].as_array().unwrap().len()
        );
        let curves = inspection["curves"].as_array().unwrap();
        assert_eq!(stats["curves"], curves.len());
        assert_eq!(stats["curves"], curve_count);
        let cvs: u64 = curves
            .iter()
            .map(|curve| curve["control_vertices"].as_u64().unwrap())
            .sum();
        assert_eq!(stats["control_vertices"], cvs);
        assert_eq!(cvs, cv_count);
    }
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
