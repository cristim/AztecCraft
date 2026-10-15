use crate::editor_operations::Operation;
use crate::viewport_renderer::{RenderError, SelectionFeedback, ViewportRenderer};
use azteccraft::{
    native_scene::{FaceTarget, NativeId},
    scene::NodeId,
    view_scene::{ViewError, ViewScene},
    viewport::{Camera, InvalidCameraInput},
};
use eframe::egui::{self, Color32, Painter, PointerButton, Rect};
use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionRefusal {
    ModifiedClick,
    AmbiguousPrimaryInput,
}

impl SelectionRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::ModifiedClick => {
                "Only plain primary-click selection is supported; Shift/Ctrl selection is unsupported"
            }
            Self::AmbiguousPrimaryInput => {
                "Ambiguous coalesced primary input cannot select; retry with a single click"
            }
        }
    }
}

pub struct ViewportInput {
    painter: Painter,
    rect: Rect,
    physical_size: [u32; 2],
    click_ndc: Option<Vec2>,
    click_points: Option<Vec2>,
    pixels_per_point: f32,
    unsupported_selection: Option<SelectionRefusal>,
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
    let (delta, modifiers, scroll) =
        ui.input(|i| (i.pointer.delta(), i.modifiers, i.smooth_scroll_delta.y));
    let alt = modifiers.alt;
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
    let mut click_points = None;
    let mut unsupported_selection = None;
    let (released, ambiguous, only_alt) = ui.input(|input| {
        let mut releases = input
            .events
            .iter()
            .rev()
            .filter_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers,
                } => Some((*pos, *modifiers)),
                _ => None,
            })
            .peekable();
        let released = releases.next();
        let ambiguous = releases.peek().is_some();
        let only_alt = released.is_some_and(|(_, modifiers)| modifiers.alt)
            && releases.all(|(_, modifiers)| modifiers.alt);
        (released, ambiguous, only_alt)
    });
    if response.clicked_by(PointerButton::Primary) && ambiguous && !only_alt {
        unsupported_selection = Some(SelectionRefusal::AmbiguousPrimaryInput);
    } else if response.clicked_by(PointerButton::Primary)
        && !ambiguous
        && let Some((point, modifiers)) = released
        && !modifiers.alt
        && rect.contains(point)
    {
        let p = (point - rect.min) / rect.size();
        let ndc = Vec2::new(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0);
        if !ndc.is_finite() {
            return Err(InvalidCameraInput);
        }
        click_ndc = Some(ndc);
        click_points = Some(Vec2::new(point.x - rect.min.x, point.y - rect.min.y));
        if modifiers.shift || modifiers.ctrl || modifiers.command {
            unsupported_selection = Some(SelectionRefusal::ModifiedClick);
        }
    }
    Ok(Some(ViewportInput {
        painter,
        rect,
        physical_size: [pixels.width_px as u32, pixels.height_px as u32],
        click_ndc,
        click_points,
        pixels_per_point: scale,
        unsupported_selection,
        navigation,
    }))
}

impl ViewportInput {
    pub fn aspect(&self) -> f32 {
        self.physical_size[0] as f32 / self.physical_size[1] as f32
    }
    pub fn click_ndc(&self) -> Option<Vec2> {
        self.click_ndc
    }
    pub fn unsupported_selection(&self) -> Option<SelectionRefusal> {
        self.unsupported_selection
    }
    pub fn pick_face_center(
        &self,
        camera: &Camera,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
    ) -> Result<Option<FaceTarget>, ViewError> {
        self.click_points
            .map(|point| {
                camera.pick_face_center(
                    scene,
                    active,
                    point,
                    self.physical_size,
                    self.pixels_per_point,
                )
            })
            .transpose()
            .map(Option::flatten)
    }
    pub fn take_navigation(&mut self) -> Vec<Operation> {
        std::mem::take(&mut self.navigation)
    }

    pub fn paint(
        &self,
        renderer: Option<&ViewportRenderer>,
        camera: &Camera,
        scene: &ViewScene,
        selection: SelectionFeedback<'_>,
        grid_visible: bool,
    ) -> Result<(), RenderError> {
        let painter = &self.painter;
        let rect = self.rect;
        painter.rect_filled(rect, 0.0, Color32::from_gray(31));
        let callback = renderer
            .ok_or(RenderError::Uninitialized)
            .and_then(|renderer| {
                renderer.callback(
                    scene,
                    camera,
                    rect,
                    self.physical_size,
                    selection,
                    grid_visible,
                )
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
