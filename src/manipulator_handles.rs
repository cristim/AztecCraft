//! Screen-space handles for the Rotate and Scale tools, built on the Move handle capture.
//!
//! The caller supplies the handle frame: the pivot and the three world-space axis directions of
//! the active space (the node's own axes in Object space, the world axes in World space).
//! Sizes in pixels, the edge-on limit and the uniform-scale reference are provisional until
//! checked against the pinned Maya reference.

use crate::{
    move_handles::{
        HIT_RADIUS, HandleError, ScreenSegment, ViewportCapture, axis_parameter_along,
        clip_segment, segment_distance_squared,
    },
    native_scene::{RotateAxis, ScaleHandle},
};
use glam::{DVec2, DVec3, Vec2};
use std::{f64::consts::TAU, fmt};

#[cfg(feature = "desktop")]
pub mod paint;

const RING_SEGMENTS: usize = 64;
const DISABLED_SEGMENTS: usize = 48;
const VIEW_RING_FACTOR: f64 = 1.2;
// |ray . ring normal| below this is an edge-on ring (about 0.06 degrees from the plane).
const EDGE_ON_LIMIT: f64 = 1e-3;
// Largest dot product between two unit frame axes (about 6e-5 degrees from perpendicular).
const MAX_SKEW: f64 = 1e-6;
const BOX_HALF: f64 = 5.0;
const CENTER_BOX_HALF: f64 = 7.0;
// A Scale pivot grab closer than this to the pivot has no meaningful ratio.
const MIN_BEGIN_PIXELS: f64 = 1.0;
// Uniform scale doubles after the pointer moves this far right or up.
const UNIFORM_REFERENCE_PIXELS: f64 = 100.0;
const PLANE_NEAR: f64 = 0.3;
const PLANE_FAR: f64 = 0.5;

/// Pivot and axis directions of the space a manipulator works in.
#[derive(Clone, Copy, Debug)]
pub struct HandleFrame {
    origin: DVec3,
    axes: [DVec3; 3],
}

impl HandleFrame {
    /// `axes` must be perpendicular (their lengths may differ) and right-handed, as a rotation of
    /// the world axes. A mirrored (left-handed) frame would flip the sign of every ring drag and a
    /// skewed one would draw rings that do not match the edit, so both are refused.
    pub fn new(origin: DVec3, axes: [DVec3; 3]) -> Result<Self, HandleError> {
        let mut unit = [DVec3::ZERO; 3];
        for (slot, axis) in unit.iter_mut().zip(axes) {
            let length = axis.length();
            if !origin.is_finite() || !axis.is_finite() || !length.is_finite() || length == 0.0 {
                return Err(HandleError::InvalidProjection);
            }
            *slot = axis / length;
        }
        let [x, y, z] = unit;
        if x.dot(y).abs() > MAX_SKEW || y.dot(z).abs() > MAX_SKEW || z.dot(x).abs() > MAX_SKEW {
            return Err(HandleError::SkewedFrame);
        }
        if x.cross(y).dot(z) <= 0.0 {
            return Err(HandleError::MirroredFrame);
        }
        Ok(Self { origin, axes: unit })
    }

    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    pub fn world(origin: DVec3) -> Result<Self, HandleError> {
        Self::new(origin, [DVec3::X, DVec3::Y, DVec3::Z])
    }
}

/// Handles that are drawn but cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisabledKind {
    ViewRing,
    FreeRotate,
    ScalePlane,
}

impl fmt::Display for DisabledKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ViewRing => "View-axis rotation is unsupported",
            Self::FreeRotate => "Free rotation is unsupported",
            Self::ScalePlane => "Plane scaling is unsupported",
        })
    }
}

/// What lies under the pointer: an enabled handle, or a drawn-but-unsupported one that no enabled
/// handle outranks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick<H> {
    Handle(H),
    Disabled(DisabledKind),
}

impl<H> Pick<H> {
    pub fn handle(self) -> Option<H> {
        match self {
            Self::Handle(handle) => Some(handle),
            Self::Disabled(_) => None,
        }
    }

    pub fn disabled(self) -> Option<DisabledKind> {
        match self {
            Self::Handle(_) => None,
            Self::Disabled(kind) => Some(kind),
        }
    }
}

pub struct DisabledHandle {
    pub kind: DisabledKind,
    segments: Vec<ScreenSegment>,
}

impl DisabledHandle {
    pub fn segments(&self) -> &[ScreenSegment] {
        &self.segments
    }
}

fn inside_viewport(pixel: DVec2, size: DVec2) -> bool {
    pixel.is_finite() && !pixel.cmplt(DVec2::ZERO).any() && !pixel.cmpgt(size).any()
}

/// The disabled handle with the nearest segment inside the pick threshold; ties go to the earlier
/// handle, like the enabled handles.
fn nearest_disabled(handles: &[DisabledHandle], pixel: DVec2) -> Option<DisabledKind> {
    let mut closest = HIT_RADIUS * HIT_RADIUS;
    let mut selected = None;
    for handle in handles {
        for segment in &handle.segments {
            let distance = segment_distance_squared(*segment, pixel);
            if distance <= closest && (selected.is_none() || distance < closest) {
                closest = distance;
                selected = Some(handle.kind);
            }
        }
    }
    selected
}

fn polyline(points: &[DVec2], size: DVec2) -> Vec<ScreenSegment> {
    points
        .windows(2)
        .filter_map(|pair| clip_segment(pair[0], pair[1], size))
        .collect()
}

fn screen_circle(center: DVec2, radius: f64, size: DVec2) -> Vec<ScreenSegment> {
    let points: Vec<_> = (0..=DISABLED_SEGMENTS)
        .map(|step| {
            let angle = TAU * step as f64 / DISABLED_SEGMENTS as f64;
            center + DVec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    polyline(&points, size)
}

fn square(center: DVec2, half: f64, size: DVec2) -> [Option<ScreenSegment>; 4] {
    let corners = [
        center + DVec2::new(-half, -half),
        center + DVec2::new(half, -half),
        center + DVec2::new(half, half),
        center + DVec2::new(-half, half),
    ];
    [0, 1, 2, 3].map(|index| clip_segment(corners[index], corners[(index + 1) % 4], size))
}

pub struct RotateRing {
    pub axis: RotateAxis,
    segments: Vec<ScreenSegment>,
}

impl RotateRing {
    /// The camera-facing half of the ring; the back half is neither drawn nor pickable.
    pub fn segments(&self) -> &[ScreenSegment] {
        &self.segments
    }
}

pub struct RotateHandles {
    origin: DVec2,
    size: DVec2,
    rings: [RotateRing; 3],
    disabled: Vec<DisabledHandle>,
}

const ROTATE_AXES: [RotateAxis; 3] = [RotateAxis::X, RotateAxis::Y, RotateAxis::Z];

// The ring plane basis (u, v) with u x v along the axis, so a positive angle turns right-handed.
fn ring_basis(index: usize) -> (usize, usize) {
    ((index + 1) % 3, (index + 2) % 3)
}

fn axis_index(axis: RotateAxis) -> usize {
    match axis {
        RotateAxis::X => 0,
        RotateAxis::Y => 1,
        RotateAxis::Z => 2,
    }
}

impl RotateHandles {
    pub fn new(capture: &ViewportCapture, frame: &HandleFrame) -> Result<Self, HandleError> {
        let (clip, origin) = capture.project_origin(frame.origin)?;
        let size = capture.size();
        let radius = capture.world_length(capture.handle_length(), clip.w);
        let to_eye = capture.eye() - frame.origin;
        if !radius.is_finite() || !to_eye.is_finite() {
            return Err(HandleError::InvalidProjection);
        }
        let rings = ROTATE_AXES.map(|axis| {
            let (u, v) = ring_basis(axis_index(axis));
            let normal =
                |step: f64| frame.axes[u] * (TAU * step).cos() + frame.axes[v] * (TAU * step).sin();
            let mut segments = Vec::new();
            for step in 0..RING_SEGMENTS {
                let from = step as f64 / RING_SEGMENTS as f64;
                let to = (step + 1) as f64 / RING_SEGMENTS as f64;
                if normal((from + to) * 0.5).dot(to_eye) <= 0.0 {
                    continue;
                }
                let points = [from, to].map(|fraction| {
                    capture.project_point(frame.origin + normal(fraction) * radius)
                });
                if let [Some(start), Some(end)] = points
                    && let Some(segment) = clip_segment(start, end, size)
                {
                    segments.push(segment);
                }
            }
            RotateRing { axis, segments }
        });
        let disabled = vec![
            DisabledHandle {
                kind: DisabledKind::FreeRotate,
                segments: screen_circle(origin, capture.handle_length(), size),
            },
            DisabledHandle {
                kind: DisabledKind::ViewRing,
                segments: screen_circle(origin, capture.handle_length() * VIEW_RING_FACTOR, size),
            },
        ];
        Ok(Self {
            origin,
            size,
            rings,
            disabled,
        })
    }

    pub fn rings(&self) -> &[RotateRing; 3] {
        &self.rings
    }

    pub fn disabled(&self) -> &[DisabledHandle] {
        &self.disabled
    }

    pub fn origin(&self) -> DVec2 {
        self.origin
    }

    /// The ring with the nearest visible segment inside the pick threshold; ties go to X, Y, Z.
    pub fn hit_test(&self, pixel: DVec2) -> Option<RotateAxis> {
        if !inside_viewport(pixel, self.size) {
            return None;
        }
        let mut closest = HIT_RADIUS * HIT_RADIUS;
        let mut selected = None;
        for ring in &self.rings {
            for segment in &ring.segments {
                let distance = segment_distance_squared(*segment, pixel);
                if distance <= closest && (selected.is_none() || distance < closest) {
                    closest = distance;
                    selected = Some(ring.axis);
                }
            }
        }
        selected
    }

    /// The one pick for hover, press and inspection: an enabled ring wins, else a disabled handle.
    pub fn pick(&self, pixel: DVec2) -> Option<Pick<RotateAxis>> {
        if !inside_viewport(pixel, self.size) {
            return None;
        }
        match self.hit_test(pixel) {
            Some(axis) => Some(Pick::Handle(axis)),
            None => nearest_disabled(&self.disabled, pixel).map(Pick::Disabled),
        }
    }

    /// A drawn-but-unsupported handle under the pointer, for the status line.
    pub fn hit_disabled(&self, pixel: DVec2) -> Option<DisabledKind> {
        self.pick(pixel).and_then(Pick::disabled)
    }
}

/// One Rotate ring drag. The angle accumulates across samples so it can pass 180 degrees.
pub struct RotateDrag {
    capture: ViewportCapture,
    origin: DVec3,
    normal: DVec3,
    u: DVec3,
    v: DVec3,
    last: f64,
    total: f64,
}

impl RotateDrag {
    pub fn begin(
        capture: ViewportCapture,
        frame: &HandleFrame,
        axis: RotateAxis,
        pointer: Vec2,
    ) -> Result<Self, HandleError> {
        let index = axis_index(axis);
        let (u, v) = ring_basis(index);
        let mut drag = Self {
            origin: frame.origin,
            normal: frame.axes[index],
            u: frame.axes[u],
            v: frame.axes[v],
            last: 0.0,
            total: 0.0,
            capture,
        };
        let pixel = drag.capture.pointer_pixel(pointer)?;
        if !drag.capture.contains(pixel) {
            return Err(HandleError::OriginClipped);
        }
        drag.last = drag.plane_angle(pixel)?;
        Ok(drag)
    }

    pub fn same_capture(&self, current_capture: &ViewportCapture) -> bool {
        self.capture.same_mapping(current_capture)
    }

    /// Angle in radians of the ray-plane hit around the ring axis.
    fn plane_angle(&self, pixel: DVec2) -> Result<f64, HandleError> {
        let ray = self.capture.ray_at_pixel(pixel)?;
        if !ray.origin.is_finite() || !ray.direction.is_finite() {
            return Err(HandleError::InvalidSolve);
        }
        let direction = ray.direction.as_dvec3();
        let length = direction.length();
        if !length.is_finite() || length <= 0.0 {
            return Err(HandleError::InvalidSolve);
        }
        let direction = direction / length;
        let denominator = direction.dot(self.normal);
        if !denominator.is_finite() || denominator.abs() < EDGE_ON_LIMIT {
            return Err(HandleError::Degenerate);
        }
        let ray_origin = ray.origin.as_dvec3();
        let parameter = (self.origin - ray_origin).dot(self.normal) / denominator;
        if !parameter.is_finite() {
            return Err(HandleError::InvalidSolve);
        }
        if parameter < 0.0 {
            return Err(HandleError::BehindRay);
        }
        let offset = ray_origin + direction * parameter - self.origin;
        let (x, y) = (offset.dot(self.u), offset.dot(self.v));
        if !x.is_finite() || !y.is_finite() || (x == 0.0 && y == 0.0) {
            return Err(HandleError::Degenerate);
        }
        Ok(y.atan2(x))
    }

    /// Signed degrees turned since the drag began. A refused sample leaves the drag unchanged.
    pub fn angle_degrees(
        &mut self,
        current_capture: &ViewportCapture,
        pointer: Vec2,
    ) -> Result<f64, HandleError> {
        if !self.capture.same_mapping(current_capture) {
            return Err(HandleError::CaptureChanged);
        }
        let pixel = self.capture.pointer_pixel(pointer)?;
        let angle = self.plane_angle(pixel)?;
        let step =
            (angle - self.last + std::f64::consts::PI).rem_euclid(TAU) - std::f64::consts::PI;
        let total = self.total + step;
        if !total.is_finite() {
            return Err(HandleError::InvalidSolve);
        }
        self.last = angle;
        self.total = total;
        Ok(total.to_degrees())
    }
}

const SCALE_HANDLES: [ScaleHandle; 3] = [ScaleHandle::X, ScaleHandle::Y, ScaleHandle::Z];

fn scale_index(handle: ScaleHandle) -> Option<usize> {
    SCALE_HANDLES
        .iter()
        .position(|candidate| *candidate == handle)
}

pub struct ScaleBox {
    pub handle: ScaleHandle,
    geometry: Result<ScaleGeometry, HandleError>,
    collapsed: bool,
}

pub struct ScaleGeometry {
    shaft: Option<ScreenSegment>,
    tip_box: [Option<ScreenSegment>; 4],
}

impl ScaleGeometry {
    pub fn segments(&self) -> impl Iterator<Item = &ScreenSegment> {
        self.shaft.iter().chain(self.tip_box.iter().flatten())
    }
}

impl ScaleBox {
    pub fn geometry(&self) -> Option<&ScaleGeometry> {
        self.geometry.as_ref().ok()
    }

    pub fn unavailable_reason(&self) -> Option<HandleError> {
        self.geometry.as_ref().err().copied()
    }
}

pub struct ScaleHandles {
    origin: DVec2,
    size: DVec2,
    axes: [ScaleBox; 3],
    center: [Option<ScreenSegment>; 4],
    disabled: Vec<DisabledHandle>,
}

impl ScaleHandles {
    pub fn new(capture: &ViewportCapture, frame: &HandleFrame) -> Result<Self, HandleError> {
        let (clip, origin) = capture.project_origin(frame.origin)?;
        let size = capture.size();
        let directions: [Result<(DVec2, f64), HandleError>; 3] = [0, 1, 2].map(|index| {
            let axis = frame.axes[index];
            axis_parameter_along(frame.origin, axis, capture.ray_at_pixel(origin)?)?;
            capture.axis_screen_vector(frame.origin, clip, axis)
        });
        let axes = [0, 1, 2].map(|index| {
            let collapsed = directions[index].is_ok_and(|(direction, length)| {
                (direction * length).abs().max_element() <= CENTER_BOX_HALF
            });
            let geometry = directions[index].map(|(direction, length)| {
                let tip = origin + direction * length;
                let shaft_end = origin + direction * (length - BOX_HALF).max(0.0);
                ScaleGeometry {
                    shaft: clip_segment(origin, shaft_end, size),
                    tip_box: square(tip, BOX_HALF, size),
                }
            });
            let geometry = geometry.and_then(|geometry| {
                if geometry
                    .segments()
                    .any(|segment| segment.start != segment.end)
                {
                    Ok(geometry)
                } else {
                    Err(HandleError::HandleClipped)
                }
            });
            ScaleBox {
                handle: SCALE_HANDLES[index],
                geometry,
                collapsed,
            }
        });
        let disabled = [(0, 1), (1, 2), (2, 0)]
            .into_iter()
            .filter_map(|(a, b)| {
                let (da, db) = (directions[a].ok()?.0, directions[b].ok()?.0);
                let corner = |s: f64, t: f64| origin + (da * s + db * t) * capture.handle_length();
                let points = [
                    corner(PLANE_NEAR, PLANE_NEAR),
                    corner(PLANE_FAR, PLANE_NEAR),
                    corner(PLANE_FAR, PLANE_FAR),
                    corner(PLANE_NEAR, PLANE_FAR),
                    corner(PLANE_NEAR, PLANE_NEAR),
                ];
                Some(DisabledHandle {
                    kind: DisabledKind::ScalePlane,
                    segments: polyline(&points, size),
                })
            })
            .collect();
        Ok(Self {
            origin,
            size,
            axes,
            center: square(origin, CENTER_BOX_HALF, size),
            disabled,
        })
    }

    pub fn axes(&self) -> &[ScaleBox; 3] {
        &self.axes
    }

    pub fn center(&self) -> impl Iterator<Item = &ScreenSegment> {
        self.center.iter().flatten()
    }

    pub fn disabled(&self) -> &[DisabledHandle] {
        &self.disabled
    }

    /// The center box wins inside its square; otherwise the nearest axis box or shaft, leaving out
    /// an axis collapsed into the center box.
    pub fn hit_test(&self, pixel: DVec2) -> Option<ScaleHandle> {
        if !inside_viewport(pixel, self.size) {
            return None;
        }
        if (pixel - self.origin).abs().max_element() <= CENTER_BOX_HALF {
            return Some(ScaleHandle::Uniform);
        }
        let mut closest = HIT_RADIUS * HIT_RADIUS;
        let mut selected = None;
        for handle in self.axes.iter().filter(|handle| !handle.collapsed) {
            if let Some(geometry) = handle.geometry() {
                for segment in geometry.segments() {
                    let distance = segment_distance_squared(*segment, pixel);
                    if distance <= closest && (selected.is_none() || distance < closest) {
                        closest = distance;
                        selected = Some(handle.handle);
                    }
                }
            }
        }
        selected
    }

    /// The one pick for hover, press and inspection: an enabled handle wins, else a disabled one.
    pub fn pick(&self, pixel: DVec2) -> Option<Pick<ScaleHandle>> {
        if !inside_viewport(pixel, self.size) {
            return None;
        }
        match self.hit_test(pixel) {
            Some(handle) => Some(Pick::Handle(handle)),
            None => nearest_disabled(&self.disabled, pixel).map(Pick::Disabled),
        }
    }

    pub fn hit_disabled(&self, pixel: DVec2) -> Option<DisabledKind> {
        self.pick(pixel).and_then(Pick::disabled)
    }
}

enum ScaleParameter {
    Axis { origin: DVec3, direction: DVec3 },
    Uniform { begin: DVec2 },
}

/// One Scale handle drag; the factor is the ratio of the current to the starting parameter.
pub struct ScaleDrag {
    capture: ViewportCapture,
    parameter: ScaleParameter,
    begin_parameter: f64,
}

impl ScaleDrag {
    pub fn begin(
        capture: ViewportCapture,
        frame: &HandleFrame,
        handle: ScaleHandle,
        pointer: Vec2,
    ) -> Result<Self, HandleError> {
        let pixel = capture.pointer_pixel(pointer)?;
        if !capture.contains(pixel) {
            return Err(HandleError::OriginClipped);
        }
        let Some(index) = scale_index(handle) else {
            return Ok(Self {
                capture,
                parameter: ScaleParameter::Uniform { begin: pixel },
                begin_parameter: UNIFORM_REFERENCE_PIXELS,
            });
        };
        let (clip, _) = capture.project_origin(frame.origin)?;
        let direction = frame.axes[index];
        let begin_parameter =
            axis_parameter_along(frame.origin, direction, capture.ray_at_pixel(pixel)?)?;
        let minimum = capture.world_length(MIN_BEGIN_PIXELS, clip.w);
        if !minimum.is_finite() || begin_parameter.abs() < minimum {
            return Err(HandleError::ZeroBeginParameter);
        }
        Ok(Self {
            capture,
            parameter: ScaleParameter::Axis {
                origin: frame.origin,
                direction,
            },
            begin_parameter,
        })
    }

    pub fn same_capture(&self, current_capture: &ViewportCapture) -> bool {
        self.capture.same_mapping(current_capture)
    }

    /// Current over starting parameter. It may be zero or negative; the scale core refuses those.
    pub fn factor(
        &self,
        current_capture: &ViewportCapture,
        pointer: Vec2,
    ) -> Result<f64, HandleError> {
        if !self.capture.same_mapping(current_capture) {
            return Err(HandleError::CaptureChanged);
        }
        let pixel = self.capture.pointer_pixel(pointer)?;
        let parameter = match &self.parameter {
            ScaleParameter::Axis { origin, direction } => {
                axis_parameter_along(*origin, *direction, self.capture.ray_at_pixel(pixel)?)?
            }
            ScaleParameter::Uniform { begin } => {
                let moved = pixel - *begin;
                UNIFORM_REFERENCE_PIXELS + moved.x - moved.y
            }
        };
        let factor = parameter / self.begin_parameter;
        if !factor.is_finite() {
            return Err(HandleError::InvalidSolve);
        }
        Ok(factor)
    }
}

#[cfg(test)]
mod tests;
