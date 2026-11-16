//! Scenarios for option windows and the unsaved-scene prompt.

use super::super::rig::Rig;
use super::{Captured, Scenario, panels::face_rig};
use crate::app::features::{extrude_options, primitive_options, rigging};
use azteccraft::scene::Primitive;
use eframe::egui;

/// Opens a window on `rig` with `open`, and reports what appears. `id` registers the window.
fn window(
    name: &str,
    title: &'static str,
    id: egui::Id,
    rig: impl Fn() -> Rig + 'static,
    open: impl Fn(&mut Rig) + 'static,
) -> Scenario {
    Scenario::new(name, "dialogs", move || {
        let mut rig = rig();
        let base = rig.update();
        open(&mut rig);
        rig.settle();
        Captured {
            windows: vec![id],
            window_title: Some(title),
            ..super::plain(&rig, Some(base))
        }
    })
}

pub(super) fn scenarios() -> Vec<Scenario> {
    vec![
        window(
            "window-primitive-options",
            "Polygon Cube Options",
            primitive_options::window_id(),
            || Rig::new(false),
            |rig| rig.editor.open_primitive_options(Primitive::Cube),
        ),
        window(
            "window-extrude-options",
            "Extrude Options",
            extrude_options::window_id(),
            face_rig,
            |rig| rig.editor.open_extrude_options(),
        ),
        window(
            "window-create-joints",
            "Create Joints",
            rigging::window_id(),
            || Rig::new(false),
            |rig| rig.editor.open_joint_options(),
        ),
        window(
            "window-planar-mapping",
            "Planar Mapping Options",
            egui::Id::unique("planar_options_window"),
            || Rig::new(true),
            |rig| rig.editor.uv.options_open = true,
        ),
        window(
            "window-uv-editor",
            "UV Editor",
            egui::Id::unique("uv_editor_window"),
            || Rig::new(true),
            |rig| rig.editor.uv.editor_open = true,
        ),
        Scenario::new("prompt-unsaved-scene", "dialogs", || {
            let mut rig = Rig::new(true);
            let base = rig.update();
            rig.editor.new_document().unwrap();
            rig.settle();
            super::plain(&rig, Some(base))
        }),
    ]
}
