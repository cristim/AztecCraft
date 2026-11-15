//! A real `Editor` driven frame by frame with AccessKit enabled, for the scenarios.

use crate::app::Editor;
use crate::app::tests::frame_with;
use crate::editor_operations::{ActiveTool, Operation};
use azteccraft::scene::Primitive;
use eframe::egui::{self, accesskit::TreeUpdate};

pub(super) const SIZE: egui::Vec2 = egui::vec2(1440.0, 900.0);

pub(super) struct Rig {
    pub(super) editor: Editor,
    pub(super) ctx: egui::Context,
    output: egui::FullOutput,
}

impl Rig {
    /// An empty scene, or one with a selected cube.
    pub(super) fn new(cube: bool) -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut editor = Editor {
            renderer: Some(crate::viewport_renderer::tests::renderer_handle()),
            ..Editor::default()
        };
        if cube {
            editor.create(Primitive::Cube);
        }
        let mut rig = Rig {
            editor,
            ctx,
            output: egui::FullOutput::default(),
        };
        rig.settle();
        rig
    }

    pub(super) fn frame(&mut self, events: Vec<egui::Event>) {
        self.output = frame_with(&mut self.editor, &self.ctx, SIZE, 1.0, events);
    }

    /// Three frames: the first lays out, the rest apply what it requested.
    pub(super) fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    pub(super) fn update(&self) -> TreeUpdate {
        self.output
            .platform_output
            .accesskit_update
            .clone()
            .expect("accesskit enabled")
    }

    pub(super) fn tool(&mut self, tool: ActiveTool) {
        self.editor.native_operation(Operation::SetActiveTool(tool));
        self.settle();
    }
}
