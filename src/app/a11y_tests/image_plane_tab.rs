//! #459 P9b-1: the Attribute Editor Image Plane tab through the real Editor and real frames. Every
//! control must give the same state and the same refusal as the MCP `set_image_plane` call for the
//! same input. Text is entered by focusing the field, setting its text and blurring it, which is
//! the path a typed edit takes (`lost_focus` with changed text).
use super::rig::{Rig, SIZE};
use crate::app::features::image_plane_attributes::{FIELDS, field_id};
use crate::app::tests::click_at;
use crate::editor_operations::{
    AttributeEditorTab, Operation, OperationReply, RemoteRequest, SidebarPanel,
};
use azteccraft::image_plane::model::{
    Coverage, CoverageParts, ImagePlaneEdit, PlateDisplayMode, PlateFit,
};
use eframe::egui::{self, accesskit::Role};
use std::path::{Path, PathBuf};

pub(super) const NAME: usize = 0;
pub(super) const OFFSET: usize = 1;
pub(super) const CACHE: usize = 2;
pub(super) const GAIN: usize = 3;
pub(super) const DEPTH: usize = 4;
pub(super) const WIDTH: usize = 5;
pub(super) const HEIGHT: usize = 6;
pub(super) const ORIGIN_X: usize = 7;
pub(super) const ORIGIN_Y: usize = 8;

pub(super) fn png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![0u8; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    path
}

/// A rig with the Attribute Editor open on the Image Plane tab of an 8 x 4 plate.
pub(super) fn tab_rig(dir: &Path) -> Rig {
    let mut rig = Rig::new(true);
    rig.editor.run_import_image(png(dir, "plate.png", 8, 4));
    for operation in [
        Operation::SetSidebarPanelVisible {
            panel: SidebarPanel::AttributeEditor,
            visible: true,
        },
        Operation::SelectAttributeEditorTab {
            tab: AttributeEditorTab::ImagePlane,
        },
    ] {
        rig.editor.native_operation(operation);
    }
    rig.settle();
    rig
}

pub(super) fn mcp(
    editor: &mut crate::app::Editor,
    edit: ImagePlaneEdit,
) -> Result<OperationReply, crate::editor_operations::OperationError> {
    let token = editor.scene_token;
    editor.remote_operation(
        RemoteRequest {
            expected_scene_token: Some(token),
            operation: Operation::SetImagePlane(edit),
        },
        &egui::Context::default(),
    )
}

/// Types `text` into text field `index` and blurs it.
pub(super) fn edit_text(rig: &mut Rig, index: usize, text: &str) {
    rig.ctx.memory_mut(|m| m.request_focus(field_id(index)));
    rig.settle();
    rig.editor.plane_drafts[index] = text.to_owned();
    rig.frame(vec![]);
    rig.ctx.memory_mut(|m| m.surrender_focus(field_id(index)));
    rig.settle();
}

pub(super) fn node_pos(rig: &Rig, role: Role, label: &str) -> egui::Pos2 {
    rig.update()
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role && node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        .next()
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"))
}

pub(super) fn click_node(rig: &mut Rig, role: Role, label: &str) {
    let at = node_pos(rig, role, label);
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
}

pub(super) fn plane(rig: &Rig) -> azteccraft::image_plane::model::ImagePlane {
    rig.editor.image_plane.as_ref().unwrap().plane.clone()
}

#[test]
fn each_text_control_gives_the_same_state_as_the_mcp_call() {
    let dir = tempfile::tempdir().unwrap();
    let other = png(dir.path(), "other.png", 8, 4);
    let cases: [(usize, String, ImagePlaneEdit); 9] = [
        (
            NAME,
            other.to_string_lossy().into_owned(),
            ImagePlaneEdit {
                image_name: Some(other.clone()),
                ..Default::default()
            },
        ),
        (
            OFFSET,
            "-3".into(),
            ImagePlaneEdit {
                frame_offset: Some(-3),
                ..Default::default()
            },
        ),
        (
            CACHE,
            "10".into(),
            ImagePlaneEdit {
                frame_cache: Some(10),
                ..Default::default()
            },
        ),
        (
            GAIN,
            "0.5".into(),
            ImagePlaneEdit {
                alpha_gain: Some(0.5),
                ..Default::default()
            },
        ),
        (
            DEPTH,
            "250.5".into(),
            ImagePlaneEdit {
                depth: Some(250.5),
                ..Default::default()
            },
        ),
        (
            WIDTH,
            "4".into(),
            ImagePlaneEdit {
                coverage_parts: CoverageParts {
                    width: Some(4),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            HEIGHT,
            "2".into(),
            ImagePlaneEdit {
                coverage_parts: CoverageParts {
                    height: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            ORIGIN_X,
            "2".into(),
            ImagePlaneEdit {
                coverage_parts: CoverageParts {
                    origin_x: Some(2),
                    width: Some(4),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
        (
            ORIGIN_Y,
            "1".into(),
            ImagePlaneEdit {
                coverage_parts: CoverageParts {
                    origin_y: Some(1),
                    height: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            },
        ),
    ];
    for (index, text, edit) in cases {
        let mut ui = tab_rig(dir.path());
        let mut twin = crate::app::Editor::default();
        twin.run_import_image(dir.path().join("plate.png"));
        // Coverage origin cases need room: start both from a 4 x 2 region at (0, 0).
        if index >= ORIGIN_X {
            let region = ImagePlaneEdit {
                coverage_parts: CoverageParts {
                    width: Some(4),
                    height: Some(2),
                    ..Default::default()
                },
                ..Default::default()
            };
            mcp(&mut ui.editor, region.clone()).unwrap();
            mcp(&mut twin, region).unwrap();
            ui.settle();
        }
        edit_text(&mut ui, index, &text);
        mcp(&mut twin, edit).unwrap();
        assert_eq!(
            ui.editor.image_plane, twin.image_plane,
            "field {index}: status {:?}",
            ui.editor.status
        );
        assert!(!ui.editor.error, "field {index}: {}", ui.editor.status);
    }
}

#[test]
fn an_untouched_text_field_never_rewrites_a_value() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    assert!(plane(&rig).coverage.is_none());
    // The coverage fields show the full image; blurring them untouched must not store it.
    for index in [WIDTH, HEIGHT, ORIGIN_X, ORIGIN_Y] {
        rig.ctx.memory_mut(|m| m.request_focus(field_id(index)));
        rig.settle();
        rig.ctx.memory_mut(|m| m.surrender_focus(field_id(index)));
        rig.settle();
        assert!(
            plane(&rig).coverage.is_none(),
            "field {index} stored a coverage"
        );
    }
    assert_eq!(plane(&rig).depth, 100.0);
}

#[test]
fn invalid_or_refused_text_names_the_field_and_keeps_the_value() {
    let dir = tempfile::tempdir().unwrap();
    for (index, bad, named) in [
        (OFFSET, "abc", "Frame Offset (frames)"),
        (CACHE, "1.5", "Frame Cache (frames)"),
        (GAIN, "x", "Alpha Gain"),
        (DEPTH, "", "Depth (cm)"),
        (WIDTH, "-1", "Coverage Width (pixels)"),
        (ORIGIN_Y, "1e2", "Coverage Origin Y (pixels)"),
        // Parses but the operation refuses it, naming its own field.
        (CACHE, "5000", "frame_cache"),
        (DEPTH, "-5", "depth"),
        (WIDTH, "9", "coverage"),
        (NAME, "x.txt", ".png"),
    ] {
        let mut rig = tab_rig(dir.path());
        let before = rig.editor.image_plane.clone();
        edit_text(&mut rig, index, bad);
        assert!(rig.editor.error, "{named}");
        assert!(
            rig.editor.status.contains(named),
            "{named}: {}",
            rig.editor.status
        );
        assert_eq!(rig.editor.image_plane, before, "{named}");
        // The field shows the stored value again.
        rig.settle();
        assert_ne!(rig.editor.plane_drafts[index], bad, "{named}");
    }
}

#[test]
fn the_combos_and_the_sequence_checkbox_send_the_typed_operation() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    click_node(&mut rig, Role::CheckBox, "Use Image Sequence");
    assert!(plane(&rig).use_frame_extension);
    click_node(&mut rig, Role::ComboBox, "Display Mode");
    let at = node_pos_label(&rig, "Luminance");
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
    assert_eq!(plane(&rig).display_mode, PlateDisplayMode::Luminance);
    click_node(&mut rig, Role::ComboBox, "Fit");
    let at = node_pos_label(&rig, "To Size");
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
    assert_eq!(plane(&rig).fit, PlateFit::ToSize);
    // The same result as the MCP call for the same values.
    let mut twin = crate::app::Editor::default();
    twin.run_import_image(dir.path().join("plate.png"));
    mcp(
        &mut twin,
        ImagePlaneEdit {
            use_frame_extension: Some(true),
            display_mode: Some(PlateDisplayMode::Luminance),
            fit: Some(PlateFit::ToSize),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(rig.editor.image_plane, twin.image_plane);
}

fn node_pos_label(rig: &Rig, label: &str) -> egui::Pos2 {
    rig.update()
        .nodes
        .iter()
        .filter(|(_, node)| node.label() == Some(label))
        .filter_map(|(_, node)| node.bounds())
        .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
        .next()
        .unwrap_or_else(|| panic!("no node named {label:?}"))
}

#[test]
fn reset_coverage_sends_the_reset_form_and_is_disabled_when_the_full_image_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = tab_rig(dir.path());
    let reset = |rig: &Rig| {
        let update = rig.update();
        let (_, node) = update
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some("Reset Coverage"))
            .expect("Reset Coverage button");
        (
            node.is_disabled(),
            node.description().unwrap_or("").to_owned(),
        )
    };
    assert_eq!(
        reset(&rig),
        (true, "Unavailable: The full image is already used".into())
    );
    mcp(
        &mut rig.editor,
        ImagePlaneEdit {
            coverage: Some(Some(Coverage {
                origin_x: 1,
                origin_y: 1,
                width: 4,
                height: 2,
            })),
            ..Default::default()
        },
    )
    .unwrap();
    rig.settle();
    assert_eq!(reset(&rig), (false, String::new()));
    click_node(&mut rig, Role::Button, "Reset Coverage");
    assert!(plane(&rig).coverage.is_none());
    let mut twin = crate::app::Editor::default();
    twin.run_import_image(dir.path().join("plate.png"));
    assert_eq!(rig.editor.image_plane, twin.image_plane);
}

#[test]
fn the_menu_row_opens_the_tab_and_every_field_is_named_with_its_unit() {
    let dir = tempfile::tempdir().unwrap();
    let mut rig = Rig::new(true);
    rig.editor
        .run_import_image(png(dir.path(), "plate.png", 8, 4));
    rig.settle();
    click_node(&mut rig, Role::Button, "View");
    rig.hover("Image Plane");
    rig.settle();
    let at = node_pos_label(&rig, "Image Plane Attributes");
    click_at(&mut rig.editor, &rig.ctx, SIZE, at);
    rig.settle();
    assert_eq!(
        rig.editor.attribute_editor_tab,
        Some(AttributeEditorTab::ImagePlane)
    );
    for label in [
        "Image Name",
        "Frame Offset (frames)",
        "Frame Cache (frames)",
        "Alpha Gain",
        "Depth (cm)",
        "Coverage Width (pixels)",
        "Coverage Height (pixels)",
        "Coverage Origin X (pixels)",
        "Coverage Origin Y (pixels)",
    ] {
        node_pos(&rig, Role::TextInput, label);
    }
    assert_eq!(FIELDS, 9);
    node_pos(&rig, Role::CheckBox, "Use Image Sequence");
    node_pos(&rig, Role::ComboBox, "Display Mode");
    node_pos(&rig, Role::ComboBox, "Fit");
}
