//! Scenarios for the docked panels: Channel Box, Attribute Editor and the Time Slider.

use super::super::rig::Rig;
use super::{Scenario, default_frame, plain};
use crate::editor_operations::{AttributeEditorTab, Operation, SelectionMode, SidebarPanel};
use azteccraft::native_scene::{ALL_CHANNELS, FaceTarget};

/// A cube in Face mode with face 0 picked, so Extrude is enabled.
pub(in crate::app::a11y_tests) fn face_rig() -> Rig {
    let mut rig = Rig::new(true);
    let id = rig.editor.single_target().unwrap().0;
    let shape = rig.editor.history.scene().shapes()[0].id();
    let transform = rig.editor.history.scene().node(id).unwrap().transform_id();
    rig.editor
        .native_operation(Operation::SetSelectionMode(SelectionMode::Face));
    rig.editor.native_operation(Operation::SelectFace {
        target: Some(FaceTarget {
            transform: id,
            shape,
            path: vec![transform, shape],
            face_index: 0,
        }),
    });
    rig.settle();
    rig
}

fn attribute_editor(name: &str, tab: fn(&Rig) -> AttributeEditorTab) -> Scenario {
    Scenario::new(name, "fields", move || {
        let mut rig = face_rig();
        rig.editor
            .native_operation(Operation::SetSidebarPanelVisible {
                panel: SidebarPanel::AttributeEditor,
                visible: true,
            });
        let tab = tab(&rig);
        rig.editor
            .native_operation(Operation::SelectAttributeEditorTab { tab });
        rig.settle();
        plain(&rig, Some(default_frame()))
    })
}

pub(super) fn scenarios() -> Vec<Scenario> {
    let node = |rig: &Rig| AttributeEditorTab::Transform(rig.editor.single_target().unwrap().0);
    let shape = |rig: &Rig| AttributeEditorTab::Shape(rig.editor.history.scene().shapes()[0].id());
    vec![
        Scenario::new("channel-box-face", "fields", || {
            plain(&face_rig(), Some(default_frame()))
        }),
        attribute_editor("attribute-editor-transform", node),
        attribute_editor("attribute-editor-shape", shape),
        Scenario::new("attribute-editor-image-plane", "fields", || {
            let dir = tempfile::tempdir().unwrap();
            let mut rig = face_rig();
            rig.editor.run_import_image(super::tiny_png(dir.path()));
            rig.editor
                .native_operation(Operation::SetSidebarPanelVisible {
                    panel: SidebarPanel::AttributeEditor,
                    visible: true,
                });
            rig.editor
                .native_operation(Operation::SelectAttributeEditorTab {
                    tab: AttributeEditorTab::ImagePlane,
                });
            rig.settle();
            plain(&rig, Some(default_frame()))
        }),
        Scenario::new("timeline-keys", "timeline", || {
            let mut rig = Rig::new(true);
            for time in [1.0, 25.0] {
                rig.editor
                    .native_operation(Operation::SetCurrentTime { time });
                rig.editor.native_operation(Operation::SetKeyframe {
                    nodes: None,
                    time: None,
                    channels: ALL_CHANNELS,
                });
            }
            rig.editor.native_operation(Operation::Play);
            rig.settle();
            plain(&rig, Some(default_frame()))
        }),
    ]
}
