use crate::editor_operations::Operation;
use crate::viewport_renderer::{RenderError, SelectionFeedback, ViewportRenderer};
use azteccraft::{
    move_handles::{HandleError, ManipulatorSize, ViewportCapture},
    native_scene::{EdgeTarget, FaceTarget, NativeId, VertexTarget},
    scene::NodeId,
    view_scene::{ViewError, ViewJoint, ViewScene},
    viewport::{Camera, InvalidCameraInput, NdcRect},
};
use eframe::egui::{self, Color32, Painter, PointerButton, Rect};
use glam::Vec2;

mod axis_lock;
use axis_lock::AxisLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectionRefusal {
    ModifiedClick,
    AmbiguousPrimaryInput,
    ComponentMarquee,
}

#[derive(Clone, Copy)]
struct PrimaryRelease {
    pos: egui::Pos2,
    modifiers: egui::Modifiers,
    clicked: bool,
}

impl PrimaryRelease {
    fn is_plain_click(&self) -> bool {
        self.clicked
            && !(self.modifiers.alt
                || self.modifiers.shift
                || self.modifiers.ctrl
                || self.modifiers.command)
    }
}

/// Primary releases of one frame in event order; a release is a click when it lies within
/// `max_click_dist` of the latest press before it in the same frame.
fn primary_releases(events: &[egui::Event], max_click_dist: f32) -> Vec<PrimaryRelease> {
    let mut press = None;
    let mut releases = Vec::new();
    for event in events {
        if let egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers,
        } = event
        {
            if *pressed {
                press = Some(*pos);
            } else {
                releases.push(PrimaryRelease {
                    pos: *pos,
                    modifiers: *modifiers,
                    clicked: press
                        .take()
                        .is_some_and(|from| from.distance(*pos) <= max_click_dist),
                });
            }
        }
    }
    releases
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
            Self::ComponentMarquee => {
                "Marquee selection in Face/Vertex/Edge mode is not implemented yet (#30)"
            }
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
    /// A finished primary drag: its rectangle (the end clamped to the viewport) and the modifiers
    /// of the release event.
    marquee: Option<(NdcRect, egui::Modifiers)>,
    /// The rubber band of a primary drag still in progress, in points inside the viewport.
    marquee_preview: Option<Rect>,
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
    let marquee_origin = response.id.with("marquee_origin");
    let marquee_cancelled = response.id.with("marquee_cancelled");
    let alt_at_press = response.id.with("alt_at_press");
    // A press begun with Alt is a camera tumble; releasing Alt before the drag crosses egui's
    // threshold must not turn it into a marquee.
    if let Some(pressed_with_alt) = ui.input(|input| {
        input.events.iter().rev().find_map(|event| match event {
            egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers,
            } if response.rect.contains(*pos) => Some(modifiers.alt),
            _ => None,
        })
    }) {
        ui.data_mut(|data| data.insert_temp(alt_at_press, pressed_with_alt));
    }
    if response.drag_started_by(PointerButton::Primary) {
        let tumbling = ui
            .data(|data| data.get_temp::<bool>(alt_at_press))
            .unwrap_or(false);
        let origin = ui
            .input(|input| input.pointer.press_origin())
            .or(response.interact_pointer_pos());
        ui.data_mut(|data| {
            data.insert_temp(orbit_in_drag, tumbling);
            data.insert_temp(marquee_cancelled, false);
            if let Some(origin) = origin {
                data.insert_temp(marquee_origin, origin);
            } else {
                data.remove::<egui::Pos2>(marquee_origin);
            }
        });
    }
    let mut navigation = Vec::with_capacity(4);
    let focused = ui.input(|input| input.focused);
    let lock_key = |button: PointerButton| response.id.with(("axis_lock", button as u8));
    for button in [PointerButton::Primary, PointerButton::Middle] {
        if !modifiers.shift || !focused || response.drag_stopped_by(button) {
            ui.data_mut(|data| data.remove::<AxisLock>(lock_key(button)));
        }
    }
    let constrained = |ui: &egui::Ui, button: PointerButton, delta: Vec2| {
        if !modifiers.shift {
            return delta;
        }
        let key = lock_key(button);
        let mut lock = ui
            .data(|data| data.get_temp::<AxisLock>(key))
            .unwrap_or_default();
        let delta = lock.constrain(delta);
        ui.data_mut(|data| data.insert_temp(key, lock));
        delta
    };
    if response.dragged_by(PointerButton::Primary) && alt {
        ui.data_mut(|data| data.insert_temp(orbit_in_drag, true));
        let delta = constrained(ui, PointerButton::Primary, Vec2::new(delta.x, delta.y));
        navigation.push(Operation::Orbit {
            radians: (Vec2::new(-delta.x, delta.y) * 0.007).to_array(),
        });
    }
    if response.dragged_by(PointerButton::Middle) && alt {
        let delta = constrained(ui, PointerButton::Middle, Vec2::new(delta.x, delta.y));
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
    let max_click_dist = ui
        .ctx()
        .options(|options| options.input_options.max_click_dist);
    let (released, ambiguous, only_alt) = ui.input(|input| {
        let releases = primary_releases(&input.events, max_click_dist);
        let released = releases
            .last()
            .map(|release| (release.pos, release.modifiers));
        let many = releases.len() > 1;
        let only_alt = released.is_some_and(|(_, modifiers)| modifiers.alt)
            && releases.iter().all(|release| release.modifiers.alt);
        // The last release decides when it is a plain click: earlier releases of the frame cannot
        // own the click, so each event keeps its own position and modifiers.
        let superseded = many && releases.last().is_some_and(PrimaryRelease::is_plain_click);
        (released, many && !superseded, only_alt)
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
    let to_ndc = |point: egui::Pos2| {
        let p = (point - rect.min) / rect.size();
        Vec2::new(p.x * 2.0 - 1.0, 1.0 - p.y * 2.0)
    };
    let origin = ui.data(|data| data.get_temp::<egui::Pos2>(marquee_origin));
    let orbiting = ui
        .data(|data| data.get_temp::<bool>(orbit_in_drag))
        .unwrap_or(false);
    // PROVISIONAL (the Maya docs are silent): Esc or losing window focus during a marquee drag
    // cancels it, the release included.
    let escape = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                }
            )
        })
    });
    let mut cancelled = ui
        .data(|data| data.get_temp::<bool>(marquee_cancelled))
        .unwrap_or(false);
    let unfocused = ui.input(|input| !input.focused);
    if (escape || unfocused)
        && (response.dragged_by(PointerButton::Primary)
            || response.drag_stopped_by(PointerButton::Primary))
    {
        cancelled = true;
        ui.data_mut(|data| data.insert_temp(marquee_cancelled, true));
    }
    let mut marquee = None;
    if !cancelled
        && !ambiguous
        && unsupported_selection.is_none()
        && response.drag_stopped_by(PointerButton::Primary)
        && let Some((end, modifiers)) = released
        && !modifiers.alt
        && !orbiting
    {
        unsupported_selection = Some(SelectionRefusal::ComponentMarquee);
        if let Some(origin) = origin {
            let rect =
                NdcRect::clamped(to_ndc(origin), to_ndc(end)).map_err(|_| InvalidCameraInput)?;
            marquee = Some((rect, modifiers));
        }
    }
    let marquee_preview =
        (response.dragged_by(PointerButton::Primary) && !alt && !orbiting && !cancelled)
            .then_some(origin)
            .flatten()
            .zip(ui.input(|input| input.pointer.latest_pos()))
            .map(|(origin, now)| Rect::from_two_pos(rect.clamp(origin), rect.clamp(now)));
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
        marquee,
        marquee_preview,
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
    pub fn marquee(&self) -> Option<(NdcRect, egui::Modifiers)> {
        self.marquee
    }
    pub fn marquee_preview(&self) -> Option<Rect> {
        self.marquee_preview
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
    pub fn pick_joint<'a>(
        &self,
        camera: &Camera,
        scene: &'a ViewScene,
    ) -> Result<Option<&'a ViewJoint>, ViewError> {
        self.click_points
            .map(|point| camera.pick_joint(scene, point, self.physical_size, self.pixels_per_point))
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
