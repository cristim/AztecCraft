use super::*;
use crate::app::features::move_tool::tests::{View, oblique, scale_plane_point, viewport};
use egui::{Event, Pos2, vec2};
use glam::Vec3;

async fn pick(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> [Value; 3] {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let viewport = &inspected["inspection"]["manipulation"]["viewport"];
    [
        viewport["pointer_hit"].clone(),
        viewport["pointer_disabled"].clone(),
        viewport["pointer_refusal"].clone(),
    ]
}

async fn hover(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    at: Pos2,
) -> [Value; 3] {
    frame(editor, context, vec![Event::PointerMoved(at)]);
    pick(editor, context, client).await
}

// The point `offset` px from `on`, perpendicular to `along`.
fn beside(on: Pos2, along: egui::Vec2, offset: f32) -> Pos2 {
    let along = along.normalized();
    on + vec2(-along.y, along.x) * offset
}

#[tokio::test]
async fn sdk_pointer_hit_follows_the_active_tool_and_matches_the_eight_pixel_pick() {
    for (tool, handle) in [("move", "x"), ("rotate", "y"), ("scale", "x")] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.renderer = Some(crate::viewport_renderer::tests::renderer_handle());
        editor.camera = oblique();
        editor.create(Primitive::Cube);
        frame(&mut editor, &context, vec![]);
        let token = editor.scene_token.to_string();
        let activated = call(
            &mut editor,
            &context,
            client.peer(),
            "set_active_tool",
            json!({"scene_token":token,"tool":tool}),
        )
        .await;
        assert!(!activated.is_error.unwrap_or(false), "{tool}");
        let output = frame(&mut editor, &context, vec![]);
        let view = View {
            camera: editor.camera.clone(),
            aspect: editor.aspect,
            rect: viewport(&output),
            scale: 1.0,
        };
        let origin = view.screen(Vec3::ZERO);
        // A point on the active tool's handle and the direction of that handle there.
        let (on, along) = match tool {
            "move" | "scale" => {
                let tip = view.screen(Vec3::X * view.radius());
                (origin + (tip - origin) * 0.5, tip - origin)
            }
            _ => (
                view.ring(Vec3::Y, 0.0),
                view.ring(Vec3::Y, 1.0) - view.ring(Vec3::Y, -1.0),
            ),
        };
        let hit = |pointer_hit: &str| [json!(pointer_hit), Value::Null, Value::Null];
        assert_eq!(
            hover(&mut editor, &context, &client, beside(on, along, 7.0)).await,
            hit(handle),
            "{tool} at 7 px"
        );
        assert_eq!(
            hover(&mut editor, &context, &client, beside(on, along, 9.0)).await,
            [Value::Null, Value::Null, Value::Null],
            "{tool} at 9 px"
        );
        match tool {
            "rotate" => assert_eq!(
                hover(
                    &mut editor,
                    &context,
                    &client,
                    origin + vec2(72.0 * 1.2, 0.0)
                )
                .await,
                [Value::Null, json!("view_ring"), Value::Null]
            ),
            "scale" => {
                assert_eq!(
                    hover(&mut editor, &context, &client, origin).await,
                    hit("uniform")
                );
                let resized = call(
                    &mut editor,
                    &context,
                    client.peer(),
                    "set_manipulator_size",
                    json!({"scene_token":token,"size":0.1}),
                )
                .await;
                assert!(!resized.is_error.unwrap_or(false));
                frame(&mut editor, &context, vec![]);
                let plane = scale_plane_point(&editor, view.rect);
                assert_eq!(
                    hover(&mut editor, &context, &client, plane).await,
                    [Value::Null, json!("scale_plane"), Value::Null]
                );
            }
            _ => {}
        }
        // Select reports nothing even over the same point.
        let selected = call(
            &mut editor,
            &context,
            client.peer(),
            "set_active_tool",
            json!({"scene_token":token,"tool":"select"}),
        )
        .await;
        assert!(!selected.is_error.unwrap_or(false));
        assert_eq!(
            hover(&mut editor, &context, &client, beside(on, along, 0.0)).await,
            [Value::Null, Value::Null, Value::Null],
            "select after {tool}"
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn sdk_pointer_refusal_reports_a_refused_handle_and_a_refused_tool() {
    let read_only = "Maya ASCII scenes are read-only in this version";
    let mirrored = "createNode transform -n \"privateParent\"; setAttr \".s\" -type \"double3\" -1 1 1;\n\
                    createNode transform -n \"privateChild\" -p \"privateParent\";\n";
    let plain = "createNode transform -n \"a\";\n";
    // (tool, scene, selected node, expected [hit, disabled, refusal] at the probe)
    for (tool, nodes, node) in [
        ("rotate", plain, "a"),
        ("scale", plain, "a"),
        ("rotate", mirrored, "privateChild"),
    ] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        let source = format!(
            "//Maya ASCII 2016 scene\nrequires maya \"2016\"; \
             currentUnit -l centimeter -a degree -t film;\n{nodes}"
        );
        let scene = azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap();
        editor.history = azteccraft::history::History::new(scene);
        let id = editor
            .history
            .scene()
            .nodes()
            .iter()
            .find(|candidate| candidate.name() == node)
            .unwrap()
            .id();
        editor.select(Some(id));
        editor.renderer = Some(crate::viewport_renderer::tests::renderer_handle());
        editor.camera = oblique();
        frame(&mut editor, &context, vec![]);
        let token = editor.scene_token.to_string();
        let activated = call(
            &mut editor,
            &context,
            client.peer(),
            "set_active_tool",
            json!({"scene_token":token,"tool":tool}),
        )
        .await;
        assert!(!activated.is_error.unwrap_or(false), "{tool}");
        let output = frame(&mut editor, &context, vec![]);
        let view = View {
            camera: editor.camera.clone(),
            aspect: editor.aspect,
            rect: viewport(&output),
            scale: 1.0,
        };
        let at = if tool == "rotate" && node == "a" {
            view.ring(Vec3::Y, 0.0)
        } else {
            view.screen(Vec3::ZERO)
        };
        let picked = hover(&mut editor, &context, &client, at).await;
        match (tool, node) {
            ("rotate", "a") => assert_eq!(picked, [json!("y"), Value::Null, json!(read_only)]),
            ("scale", _) => assert_eq!(picked, [json!("uniform"), Value::Null, json!(read_only)]),
            _ => {
                assert_eq!(picked[0], Value::Null);
                assert_eq!(picked[1], Value::Null);
                let reason = picked[2].as_str().expect("whole-tool refusal");
                assert!(reason.contains("Handle axes are mirrored"), "{reason}");
                assert!(!reason.contains("private"), "{reason}");
            }
        }
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

fn chain_scene(scale: &str) -> azteccraft::native_scene::NativeScene {
    let source = format!(
        "//Maya ASCII 2016 scene\nrequires maya \"2016\"; \
         currentUnit -l centimeter -a degree -t film;\n\
         createNode transform -n \"a\"; setAttr \".s\" -type \"double3\" {scale} {scale} {scale};\n\
         createNode transform -n \"b\" -p \"a\"; setAttr \".s\" -type \"double3\" {scale} {scale} {scale};\n\
         createNode transform -n \"leaf\" -p \"b\";\n"
    );
    azteccraft::native_scene::ascii::decode(source.into_bytes().into()).unwrap()
}

// Two ancestors scaled 1e200 have no finite product, so the leaf frame is refused as ParentTransform.
// A capture taken before they overflow survives later frames (a view build error does not clear
// `move_viewport`), so the scene is swapped in directly after the pointer was captured. Plausible
// through the MCP set_transform tool on a writable scene, not exercised here (read-only ASCII).
#[tokio::test]
async fn sdk_pointer_refusal_reports_a_scene_frame_refusal() {
    let expected = azteccraft::native_scene::NativeError::UnsupportedMove(
        azteccraft::native_scene::WorldMoveUnsupported::ParentTransform,
    )
    .to_string();
    for tool in ["rotate", "scale"] {
        let context = egui::Context::default();
        let (mut editor, client, server) = start(&context).await;
        editor.history = azteccraft::history::History::new(chain_scene("1"));
        editor.renderer = Some(crate::viewport_renderer::tests::renderer_handle());
        editor.camera = oblique();
        frame(&mut editor, &context, vec![]);
        let token = editor.scene_token.to_string();
        let activated = call(
            &mut editor,
            &context,
            client.peer(),
            "set_active_tool",
            json!({"scene_token":token,"tool":tool}),
        )
        .await;
        assert!(!activated.is_error.unwrap_or(false), "{tool}");
        let output = frame(&mut editor, &context, vec![]);
        let at = viewport(&output).center();
        frame(&mut editor, &context, vec![Event::PointerMoved(at)]);
        let scene = chain_scene("1e200");
        let leaf = scene
            .nodes()
            .iter()
            .find(|node| node.name() == "leaf")
            .unwrap()
            .id();
        editor.history = azteccraft::history::History::new(scene);
        editor.select(Some(leaf));
        let picked = pick(&mut editor, &context, &client).await;
        assert_eq!(
            picked,
            [Value::Null, Value::Null, json!(expected)],
            "{tool}"
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn sdk_pointer_refusal_is_null_for_an_accepted_handle() {
    let context = egui::Context::default();
    let (mut editor, client, server) = start(&context).await;
    editor.renderer = Some(crate::viewport_renderer::tests::renderer_handle());
    editor.camera = oblique();
    editor.create(Primitive::Cube);
    frame(&mut editor, &context, vec![]);
    let token = editor.scene_token.to_string();
    call(
        &mut editor,
        &context,
        client.peer(),
        "set_active_tool",
        json!({"scene_token":token,"tool":"rotate"}),
    )
    .await;
    let output = frame(&mut editor, &context, vec![]);
    let view = View {
        camera: editor.camera.clone(),
        aspect: editor.aspect,
        rect: viewport(&output),
        scale: 1.0,
    };
    assert_eq!(
        hover(&mut editor, &context, &client, view.ring(Vec3::Y, 0.0)).await,
        [json!("y"), Value::Null, Value::Null]
    );
    client.cancel().await.unwrap();
    drop(editor);
    server.await.unwrap();
}
