use super::*;
use crate::mcp;

mod bridge;
mod file_actions;
mod hidden_editor;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}

fn camera_operations() -> [Operation; 4] {
    [
        Operation::Orbit {
            radians: [0.2, -0.1],
        },
        Operation::Pan {
            world_units: [1.0, -2.0],
        },
        Operation::Dolly { factor: 0.5 },
        Operation::SetCameraPose {
            target: [1.0, 0.5, -2.0],
            distance: 5.0,
            yaw: 0.3,
            pitch: 0.2,
        },
    ]
}

fn frame(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1440.0, 900.0),
        )),
        events,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(false);
    viewport.occluded = Some(false);
    eframe::App::raw_input_hook(editor, context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    let mut output = context.run_ui(input, |ui| {
        eframe::App::logic(editor, ui.ctx(), &mut native_frame);
        eframe::App::ui(editor, ui, &mut native_frame);
    });
    output.textures_delta.clear();
    output
}

fn hidden_frame(
    editor: &mut Editor,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> egui::LogicOutput {
    let mut input = egui::RawInput {
        events,
        focused: false,
        ..Default::default()
    };
    let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
    viewport.minimized = Some(true);
    viewport.occluded = Some(false);
    eframe::App::raw_input_hook(editor, context, &mut input);
    let mut native_frame = eframe::Frame::_new_kittest();
    context.run_logic(&input, |ctx| {
        eframe::App::logic(editor, ctx, &mut native_frame)
    })
}

fn create_request(editor: &Editor) -> RemoteRequest {
    RemoteRequest {
        expected_scene_token: Some(editor.scene_token),
        operation: Operation::Create {
            primitive: Primitive::Cube,
            options: None,
            name: None,
        },
    }
}

#[cfg(feature = "mcp-test-client")]
mod official_client {
    use super::*;
    use rmcp::{
        ServiceExt,
        model::{CallToolRequestParams, CallToolResponse, CallToolResult},
        service::{Peer, RoleClient},
    };
    use serde_json::{Value, json};
    use std::time::Duration;

    mod binary_fixture {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_binary.rs"
        ));
    }
    mod appearance_fixture {
        use super::binary_fixture as wire;
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/support/maya_appearance.rs"
        ));
    }

    mod ascii;
    mod attribute_editor;
    mod camera;
    mod cylinder_cone;
    mod delete_selected;
    mod deselect_all;
    mod extrude;
    mod files;
    mod frame_all;
    mod frame_selected;
    mod grid;
    mod inspect_budget;
    mod inspect_paging;
    mod manipulation_mcp_tests;
    mod manipulator_size;
    mod material;
    mod node_edit;
    mod obj_export;

    mod pointer_hit;
    mod primitive_options;
    mod select_by_rule;
    mod selection;
    mod session;
    mod session_material;
    mod shading;
    mod shape_naming;
    mod sited_leaf;
    mod snapshots;
    mod sphere;
    mod status_line;
    mod tool_handle;
    mod tool_space;
    mod vertex_edge;

    async fn call_response(
        editor: &mut Editor,
        context: &egui::Context,
        peer: &Peer<RoleClient>,
        name: &'static str,
        arguments: Value,
    ) -> Result<CallToolResult, rmcp::ServiceError> {
        let calling = peer.call_tool_once(
            CallToolRequestParams::new(name).with_arguments(arguments.as_object().unwrap().clone()),
        );
        tokio::pin!(calling);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            tokio::select! {
                result = &mut calling => {
                    let CallToolResponse::Complete(result) = result? else { panic!("expected complete tool result") };
                    return Ok(result);
                }
                _ = tokio::time::sleep(Duration::from_millis(1)) => { frame(editor, context, vec![]); }
                _ = tokio::time::sleep_until(deadline) => panic!("protocol call did not complete"),
            }
        }
    }

    async fn call(
        editor: &mut Editor,
        context: &egui::Context,
        peer: &Peer<RoleClient>,
        name: &'static str,
        arguments: Value,
    ) -> CallToolResult {
        call_response(editor, context, peer, name, arguments)
            .await
            .unwrap()
    }

    async fn wait_for_queue(editor: &Editor) {
        tokio::time::timeout(Duration::from_secs(1), async {
            while editor.mcp.as_ref().unwrap().test_queue_len() == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    async fn start(
        context: &egui::Context,
    ) -> (
        Editor,
        rmcp::service::RunningService<RoleClient, ()>,
        tokio::task::JoinHandle<()>,
    ) {
        let (bridge, stream, serving) = mcp::test_connection(context.clone());
        let server = tokio::spawn(serving);
        let editor = Editor {
            mcp: Some(bridge),
            ..Default::default()
        };
        let (reader, writer) = tokio::io::split(stream);
        let client = ()
            .serve(rmcp::transport::async_rw::AsyncRwTransport::new(
                reader, writer,
            ))
            .await
            .unwrap();
        (editor, client, server)
    }
}
