use crate::editor_operations::Operation;
use crate::viewport_renderer::{RenderError, SelectionFeedback, ViewportRenderer};
use azteccraft::{
    move_handles::{HandleError, ManipulatorSize, ViewportCapture},
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
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
    Marquee,
}

impl SelectionRefusal {
    pub fn message(self) -> &'static str {
        match self {
            Self::ModifiedClick => {
                "Only plain primary-click selection is supported in component modes; Shift/Ctrl selection is unsupported"
            }
            Self::AmbiguousPrimaryInput => {
                "Ambiguous coalesced primary input cannot select; retry with a single click"
            }
            Self::Marquee => "Marquee (drag) selection is not implemented yet; click to select",
        }
    }
}

#[derive(Clone, Copy)]
pub struct MovePointer {
    pub position: Option<Vec2>,
    pub pressed: bool,
    pub released: bool,
    pub down: bool,
    pub modified: bool,
    pub focused: bool,
    pub gone: bool,
}

pub struct ViewportInput {
    painter: Painter,
    rect: Rect,
    physical_origin: [i32; 2],
    physical_size: [u32; 2],
    move_pointer: MovePointer,
    click_ndc: Option<Vec2>,
    click_points: Option<Vec2>,
    /// Modifiers of the primary release event that produced the click.
    click_modifiers: Option<egui::Modifiers>,
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
    let orbit_in_drag = response.id.with("orbit_in_drag");
    if response.drag_started_by(PointerButton::Primary) {
        ui.data_mut(|data| data.insert_temp(orbit_in_drag, false));
    }
    let mut navigation = Vec::with_capacity(4);
    if response.dragged_by(PointerButton::Primary) && alt {
        ui.data_mut(|data| data.insert_temp(orbit_in_drag, true));
        navigation.push(Operation::Orbit {
            radians: (Vec2::new(-delta.x, delta.y) * 0.007).to_array(),
        });
    }
    if response.dragged_by(PointerButton::Middle) && alt {
        navigation.push(Operation::Pan {
            world_units: (Vec2::new(-delta.x, delta.y) * camera.distance() * 0.002).to_array(),
        });
    }
    if response.dragged_by(PointerButton::Secondary) && alt {
        navigation.push(Operation::Dolly {
            factor: (-delta.y * 0.01).exp(),
        });
    }
    if response.hovered() && scroll != 0.0 {
        navigation.push(Operation::Dolly {
            factor: (-scroll * 0.002).exp(),
        });
    }
    let mut click_ndc = None;
    let mut click_points = None;
    let mut click_modifiers = None;
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
        click_modifiers = Some(modifiers);
        if modifiers.shift || modifiers.ctrl || modifiers.command {
            unsupported_selection = Some(SelectionRefusal::ModifiedClick);
        }
    }
    if unsupported_selection.is_none()
        && response.drag_stopped_by(PointerButton::Primary)
        && released.is_some_and(|(_, modifiers)| !modifiers.alt)
        && !ui
            .data(|data| data.get_temp::<bool>(orbit_in_drag))
            .unwrap_or(false)
    {
        unsupported_selection = Some(SelectionRefusal::Marquee);
    }
    let move_pointer = ui.input(|input| MovePointer {
        position: input
            .pointer
            .latest_pos()
            .map(|point| Vec2::new(point.x, point.y)),
        pressed: response.contains_pointer()
            && input.pointer.button_pressed(PointerButton::Primary),
        released: input.pointer.button_released(PointerButton::Primary),
        down: input.pointer.button_down(PointerButton::Primary),
        modified: !input.modifiers.is_none()
            || input.events.iter().any(|event| match event {
                egui::Event::ModifiersChanged(modifiers)
                | egui::Event::PointerButton { modifiers, .. }
                | egui::Event::Key { modifiers, .. }
                | egui::Event::MouseWheel { modifiers, .. } => !modifiers.is_none(),
                _ => false,
            }),
        focused: input.focused,
        gone: input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::PointerGone)),
    });
    Ok(Some(ViewportInput {
        painter,
        rect,
        physical_origin: [pixels.left_px, pixels.top_px],
        physical_size: [pixels.width_px as u32, pixels.height_px as u32],
        move_pointer,
        click_ndc,
        click_points,
        click_modifiers,
        pixels_per_point: scale,
        unsupported_selection,
        navigation,
    }))
}

impl ViewportInput {
    pub fn aspect(&self) -> f32 {
        self.physical_size[0] as f32 / self.physical_size[1] as f32
    }
    pub fn move_capture(
        &self,
        camera: &Camera,
        size: ManipulatorSize,
    ) -> Result<ViewportCapture, HandleError> {
        Ok(ViewportCapture::new(
            camera,
            self.physical_origin,
            self.physical_size,
            self.pixels_per_point,
        )?
        .with_manipulator_size(size))
    }
    pub fn move_pointer(&self) -> MovePointer {
        self.move_pointer
    }
    pub fn painter(&self) -> &Painter {
        &self.painter
    }
    pub fn rect(&self) -> Rect {
        self.rect
    }
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }
    pub fn click_ndc(&self) -> Option<Vec2> {
        self.click_ndc
    }
    pub fn click_modifiers(&self) -> Option<egui::Modifiers> {
        self.click_modifiers
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
    pub fn pick_vertex(
        &self,
        camera: &Camera,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
    ) -> Result<Option<VertexTarget>, ViewError> {
        self.click_points
            .map(|point| {
                camera.pick_vertex(
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
    pub fn pick_edge(
        &self,
        camera: &Camera,
        scene: &ViewScene,
        active: Option<(NodeId, &[NativeId])>,
    ) -> Result<Option<EdgeTarget>, ViewError> {
        self.click_points
            .map(|point| {
                camera.pick_edge(
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
        shading: crate::editor_operations::ShadingMode,
    ) -> Result<bool, RenderError> {
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
                    shading,
                )
            });
        let mut diagnostic = callback.as_ref().err().map(ToString::to_string);
        if let Ok((callback, degraded)) = &callback {
            painter.add(callback.clone());
            if *degraded {
                diagnostic = Some(FACE_CENTERS_HIDDEN.into());
            }
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
        callback.map(|(_, degraded)| degraded)
    }
}

pub const FACE_CENTERS_HIDDEN: &str = "Face centres hidden: too many faces for occlusion check";

#[cfg(test)]
#[path = "viewport_ui/tests.rs"]
mod tests;
