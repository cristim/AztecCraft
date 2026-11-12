use crate::editor_operations::Operation;
use crate::viewport_renderer::{RenderError, ViewportRenderer};
use azteccraft::{
    native_scene::NativeId,
    view_scene::ViewScene,
    viewport::{Camera, InvalidCameraInput},
};
use eframe::egui::{self, Color32, Painter, PointerButton, Rect};
use glam::Vec2;

pub struct ViewportInput {
    painter: Painter,
    rect: Rect,
    click_ndc: Option<Vec2>,
    navigation: Vec<Operation>,
}

pub fn capture(
    ui: &mut egui::Ui,
    camera: &Camera,
) -> Result<Option<ViewportInput>, InvalidCameraInput> {
    let (response, painter) =
        ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
    let scale = ui.ctx().pixels_per_point();
    let screen = ui.input(|input| input.viewport_rect().size()) * scale;
    if !response.rect.is_finite()
        || !screen.is_finite()
        || !scale.is_finite()
        || scale <= 0.0
        || screen.x < 0.0
        || screen.y < 0.0
        || screen.max_elem() >= i32::MAX as f32
    {
        return Err(InvalidCameraInput);
    }
    let pixels = egui::epaint::ViewportInPixels::from_points(
        &response.rect,
        scale,
        [screen.x.round() as u32, screen.y.round() as u32],
    );
    if pixels.width_px <= 0 || pixels.height_px <= 0 {
        return Ok(None);
    }
    let rect = Rect::from_min_size(
        egui::pos2(pixels.left_px as f32 / scale, pixels.top_px as f32 / scale),
        egui::vec2(
            pixels.width_px as f32 / scale,
            pixels.height_px as f32 / scale,
        ),
    );
    let (delta, alt, scroll) =
        ui.input(|i| (i.pointer.delta(), i.modifiers.alt, i.smooth_scroll_delta.y));
    let mut navigation = Vec::with_capacity(4);
    if response.dragged_by(PointerButton::Primary) && alt {
        navigation.push(Operation::Orbit {
            radians: (Vec2::new(-delta.x, delta.y) * 0.007).to_array(),
        });
    }
    if response.dragged_by(PointerButton::Middle) {
        navigation.push(Operation::Pan {
            world_units: (Vec2::new(-delta.x, delta.y) * camera.distance() * 0.002).to_array(),
        });
    }
    if response.dragged_by(PointerButton::Secondary) {
        navigation.push(Operation::Dolly {
            factor: (delta.y * 0.01).exp(),
        });
    }
    if response.hovered() && scroll != 0.0 {
        navigation.push(Operation::Dolly {
            factor: (-scroll * 0.002).exp(),
        });
    }
    let mut click_ndc = None;
    if response.clicked_by(PointerButton::Primary)
        && !alt
        && let Some(point) = response
            .interact_pointer_pos()
            .filter(|p| rect.contains(*p))
    {
        let p = (point - rect.min) / rect.size();
        let ndc = Vec2::new(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0);
        if !ndc.is_finite() {
            return Err(InvalidCameraInput);
        }
        click_ndc = Some(ndc);
    }
    Ok(Some(ViewportInput {
        painter,
        rect,
        click_ndc,
        navigation,
    }))
}

impl ViewportInput {
    pub fn aspect(&self) -> f32 {
        self.rect.width() / self.rect.height()
    }
    pub fn click_ndc(&self) -> Option<Vec2> {
        self.click_ndc
    }
    pub fn take_navigation(&mut self) -> Vec<Operation> {
        std::mem::take(&mut self.navigation)
    }

    pub fn paint(
        &self,
        renderer: Option<&ViewportRenderer>,
        camera: &Camera,
        scene: &ViewScene,
        selected_prefix: Option<&[NativeId]>,
        grid_visible: bool,
    ) -> Result<(), RenderError> {
        let painter = &self.painter;
        let rect = self.rect;
        painter.rect_filled(rect, 0.0, Color32::from_gray(31));
        let callback = renderer
            .ok_or(RenderError::Uninitialized)
            .and_then(|renderer| {
                renderer.callback(scene, camera, rect, selected_prefix, grid_visible)
            });
        let diagnostic = callback.as_ref().err().map(ToString::to_string);
        if let Ok(callback) = &callback {
            painter.add(callback.clone());
        }
        let appearance = if scene
            .instances()
            .iter()
            .any(|instance| instance.appearance_incomplete())
        {
            "\nAppearance incomplete: geometry preview"
        } else {
            ""
        };
        painter.text(
            rect.left_bottom() + egui::vec2(12.0, -12.0),
            egui::Align2::LEFT_BOTTOM,
            format!(
                "{} instances | Perspective\nOpaque geometry preview{appearance}{}",
                scene.instances().len(),
                diagnostic.map(|e| format!("\n{e}")).unwrap_or_default()
            ),
            egui::FontId::proportional(12.0),
            Color32::GRAY,
        );
        callback.map(|_| ())
    }
}

#[cfg(test)]
#[path = "viewport_ui/tests.rs"]
mod tests;
