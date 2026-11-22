//! #459 P9b-1: the Remove row's scene-token guard and the tab's independence from the selection.
use super::image_plane_tab::{png, tab_rig};
use super::rig::SIZE;
use crate::editor_operations::{AttributeEditorTab, Operation, OperationReply, RemoteRequest};
use eframe::egui;

/// A frame of only the viewport panel (its View menu is where the Remove row lives) with `token`
/// as the frame's scene token, so a token that no longer matches the editor's can be simulated.
fn viewport_frame(
    editor: &mut crate::app::Editor,
    ctx: &egui::Context,
    token: uuid::Uuid,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SIZE)),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| {
        editor.viewport(ui, token, None, None, false, true)
    });
    output.textures_delta.clear();
    output
}

fn pos_of(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    output
        .platform_output
        .accesskit_update
        .as_ref()
        .expect("accesskit enabled")
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        .next()
        .unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn the_remove_row_is_ignored_when_the_frames_scene_token_is_stale() {
    let dir = tempfile::tempdir().unwrap();
    for (token_matches, removed) in [(false, false), (true, true)] {
        let mut editor = crate::app::Editor::default();
        editor.run_import_image(png(dir.path(), "plate.png", 4, 4));
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let good = editor.scene_token;
        let frame_token = if token_matches {
            good
        } else {
            uuid::Uuid::nil()
        };
        let run = |editor: &mut crate::app::Editor, token, events| {
            viewport_frame(editor, &ctx, token, events)
        };
        let output = run(&mut editor, good, vec![]);
        // Open View, then the Image Plane submenu, with the real token.
        let at = pos_of(&output, "View");
        for event in [
            egui::Event::PointerMoved(at),
            click(at, true),
            click(at, false),
        ] {
            run(&mut editor, good, vec![event]);
        }
        let output = run(&mut editor, good, vec![]);
        let at = pos_of(&output, "Image Plane");
        run(&mut editor, good, vec![egui::Event::PointerMoved(at)]);
        let output = run(&mut editor, good, vec![]);
        // The click on Remove arrives in a frame whose token may be stale.
        let at = pos_of(&output, "Remove Image Plane");
        run(&mut editor, good, vec![egui::Event::PointerMoved(at)]);
        for event in [click(at, true), click(at, false)] {
            run(&mut editor, frame_token, vec![event]);
        }
        run(&mut editor, frame_token, vec![]);
        assert_eq!(
            editor.image_plane.is_none(),
            removed,
            "token_matches {token_matches}"
        );
    }
}

fn click(at: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos: at,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn the_tab_needs_a_plane_not_a_selection_and_inspect_reports_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    rig.editor.native_operation(Operation::DeselectAll);
    rig.settle();
    assert_eq!(
        rig.editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::ImagePlane)
    );
    let tab =
        serde_json::to_value(rig.editor.inspect(false).unwrap()).unwrap()["attribute_editor_tab"]
            .clone();
    assert_eq!(tab, serde_json::json!({"kind": "image_plane", "id": null}));
    // The same tab is selectable remotely, and goes away with the plane.
    let token = rig.editor.scene_token;
    let reply = rig.editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SelectAttributeEditorTab {
                tab: AttributeEditorTab::ImagePlane,
            },
        },
        &egui::Context::default(),
    );
    assert!(matches!(reply, Ok(OperationReply::Unchanged)), "{reply:?}");
    rig.editor.native_operation(Operation::RemoveImagePlane);
    rig.settle();
    assert_ne!(
        rig.editor.effective_attribute_editor_tab(),
        Some(AttributeEditorTab::ImagePlane)
    );
}
