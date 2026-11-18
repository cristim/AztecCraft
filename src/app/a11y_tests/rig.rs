//! A real `Editor` driven frame by frame with AccessKit enabled, for the scenarios.

use crate::app::Editor;
use crate::app::tests::{click_at, frame_with};
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
        ctx.all_styles_mut(|style| style.interaction.selectable_labels = false);
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

    /// Centre of the topmost node named exactly `label`.
    pub(super) fn find(&self, label: &str) -> egui::Pos2 {
        self.update()
            .nodes
            .iter()
            .filter(|(_, node)| node.label() == Some(label))
            .filter_map(|(_, node)| node.bounds())
            .min_by(|a, b| a.y0.total_cmp(&b.y0))
            .map(|b| egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .unwrap_or_else(|| panic!("no AccessKit node named {label:?}"))
    }

    pub(super) fn click(&mut self, label: &str) {
        let at = self.find(label);
        self.output = click_at(&mut self.editor, &self.ctx, SIZE, at);
    }

    /// Submenus open on hover.
    pub(super) fn hover(&mut self, label: &str) {
        let at = self.find(label);
        self.frame(vec![egui::Event::PointerMoved(at)]);
        self.settle();
    }

    pub(super) fn key(&mut self, key: egui::Key) {
        self.frame(vec![crate::app::tests::key(key)]);
    }
}
