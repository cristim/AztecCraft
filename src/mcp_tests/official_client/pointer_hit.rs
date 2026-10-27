use super::*;
use crate::app::features::move_tool::tests::{View, oblique, scale_plane_point, viewport};
use egui::{Event, Pos2, vec2};
use glam::Vec3;

async fn pick(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
) -> [Value; 2] {
    let inspected = call(editor, context, client.peer(), "inspect_scene", json!({}))
        .await
        .structured_content
        .unwrap();
    let viewport = &inspected["inspection"]["manipulation"]["viewport"];
    [
        viewport["pointer_hit"].clone(),
        viewport["pointer_disabled"].clone(),
    ]
}

async fn hover(
    editor: &mut Editor,
    context: &egui::Context,
    client: &rmcp::service::RunningService<RoleClient, ()>,
    at: Pos2,
) -> [Value; 2] {
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
        let hit = |pointer_hit: &str| [json!(pointer_hit), Value::Null];
        assert_eq!(
            hover(&mut editor, &context, &client, beside(on, along, 7.0)).await,
            hit(handle),
            "{tool} at 7 px"
        );
        assert_eq!(
            hover(&mut editor, &context, &client, beside(on, along, 9.0)).await,
            [Value::Null, Value::Null],
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
                [Value::Null, json!("view_ring")]
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
                    [Value::Null, json!("scale_plane")]
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
            [Value::Null, Value::Null],
            "select after {tool}"
        );
        client.cancel().await.unwrap();
        drop(editor);
        server.await.unwrap();
    }
}
