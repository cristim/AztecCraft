use crate::{
    native_scene::WorldAxis,
    viewport::{Camera, InvalidCameraInput, Ray},
};
use glam::{DMat4, DVec2, DVec3, DVec4, Vec2};
use std::fmt;

pub(crate) const ARROW_LENGTH: f64 = 72.0;
// PROVISIONAL floor at half the default so the smallest sizes stay pickable.
const MIN_HANDLE_LENGTH: f64 = ARROW_LENGTH / 2.0;
const SIZE_STEP: f64 = 1.25;
pub const GRID_LIMIT: i32 = 10;
// A step must change the three-decimal value a set size shows, so it clears the rounding.
const HALF_MILLI: f64 = 0.0005;
pub(crate) const HIT_RADIUS: f64 = 6.0;
const ARROWHEAD_LENGTH: f64 = 12.0;
const ARROWHEAD_HALF_WIDTH: f64 = 4.0;
const CENTER_EXCLUSION: f64 = 10.0;
const PARALLEL_LIMIT: f64 = 1e-10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleError {
    Camera(InvalidCameraInput),
    InvalidCapture,
    CaptureChanged,
    NonFinitePointer,
    InvalidProjection,
    OriginClipped,
    ProjectedAxisDegenerate,
    HandleClipped,
    ParallelAxis,
    BehindRay,
    InvalidSolve,
    Degenerate,
    ZeroBeginParameter,
    MirroredFrame,
    SkewedFrame,
}

impl From<InvalidCameraInput> for HandleError {
    fn from(error: InvalidCameraInput) -> Self {
        Self::Camera(error)
    }
}

impl fmt::Display for HandleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Camera(error) => return error.fmt(f),
            Self::InvalidCapture => "Move requires a finite nonempty viewport mapping",
            Self::CaptureChanged => "Move cancelled because the camera or viewport mapping changed",
            Self::NonFinitePointer => "Move pointer position must be finite",
            Self::InvalidProjection => "Move handle projection cannot be represented",
            Self::OriginClipped => "Move origin is outside the viewport or depth range",
            Self::ProjectedAxisDegenerate => "Move axis has no usable screen direction",
            Self::HandleClipped => "Move axis geometry is entirely clipped by the viewport",
            Self::ParallelAxis => "Move axis is too close to parallel with the camera ray",
            Self::BehindRay => "Move closest point lies behind the camera ray origin",
            Self::InvalidSolve => "Move axis distance cannot be represented",
            Self::Degenerate => "Rotate ring is edge-on to the camera",
            Self::ZeroBeginParameter => "Scale handle was grabbed too close to the pivot",
            Self::MirroredFrame => {
                "Handle axes are mirrored (a negative-scale ancestor flips the drag direction)"
            }
            Self::SkewedFrame => "Handle axes are not perpendicular (an ancestor scale is skewed)",
        })
    }
}

impl HandleError {
    /// The message with the leading tool name replaced: the shared texts are written for Move.
    pub fn describe_for(&self, tool: &str) -> String {
        let text = self.to_string();
        match text.strip_prefix("Move") {
            Some(rest) => format!("{tool}{rest}"),
            None => text,
        }
    }
}

impl std::error::Error for HandleError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManipulatorSizeError {
    NotFinite,
    OutOfRange,
}

impl fmt::Display for ManipulatorSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFinite => f.write_str("manipulator size must be a finite number"),
            Self::OutOfRange => write!(
                f,
                "manipulator size must be between {} and {}",
                ManipulatorSize::MIN,
                ManipulatorSize::MAX
            ),
        }
    }
}

impl std::error::Error for ManipulatorSizeError {}

/// Global scale of the Move, Rotate and Scale manipulators. Set values round to three decimals; key steps land on 1.25^k.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManipulatorSize(f64);

// Construction rejects NaN, so equality is total.
impl Eq for ManipulatorSize {}

impl Default for ManipulatorSize {
    fn default() -> Self {
        Self(Self::DEFAULT)
    }
}

impl ManipulatorSize {
    pub const MIN: f64 = 0.1;
    pub const MAX: f64 = 10.0;
    pub const DEFAULT: f64 = 1.0;

    pub fn new(value: f64) -> Result<Self, ManipulatorSizeError> {
        if !value.is_finite() {
            return Err(ManipulatorSizeError::NotFinite);
        }
        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(ManipulatorSizeError::OutOfRange);
        }
        Ok(Self(round_milli(value)))
    }

    pub fn get(self) -> f64 {
        self.0
    }

    /// The next 1.25^k grid value above this size, or `None` at or above the top of the grid.
    pub fn grown(self) -> Option<Self> {
        Self::grid()
            .find(|grid| *grid > self.0 + HALF_MILLI)
            .map(Self)
    }

    /// The next 1.25^k grid value below this size, or `None` at or below the bottom of the grid.
    pub fn shrunk(self) -> Option<Self> {
        Self::grid()
            .rev()
            .find(|grid| *grid < self.0 - HALF_MILLI)
            .map(Self)
    }

    // Each power comes from its integer, never from a previous f64, so steps are exact inverses.
    fn grid() -> impl DoubleEndedIterator<Item = f64> {
        (-GRID_LIMIT..=GRID_LIMIT).map(|k| SIZE_STEP.powi(k))
    }
}

fn round_milli(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

#[derive(Clone)]
pub struct ViewportCapture {
    camera: Camera,
    physical_origin: [i32; 2],
    physical_size: [u32; 2],
    pixels_per_point: f32,
    logical_min: Vec2,
    logical_size: Vec2,
    aspect: f32,
    view_projection: DMat4,
    focal_pixels: f64,
    handle_length: f64,
}

impl ViewportCapture {
    pub fn new(
        camera: &Camera,
        physical_origin: [i32; 2],
        physical_size: [u32; 2],
        pixels_per_point: f32,
    ) -> Result<Self, HandleError> {
        if !pixels_per_point.is_finite()
            || pixels_per_point <= 0.0
            || (0..2).any(|component| {
                physical_origin[component] < 0
                    || physical_size[component] == 0
                    || i64::from(physical_origin[component]) + i64::from(physical_size[component])
                        > i64::from(i32::MAX)
            })
        {
            return Err(HandleError::InvalidCapture);
        }
        let logical_min =
            Vec2::new(physical_origin[0] as f32, physical_origin[1] as f32) / pixels_per_point;
        let logical_size =
            Vec2::new(physical_size[0] as f32, physical_size[1] as f32) / pixels_per_point;
        if !logical_min.is_finite()
            || !logical_size.is_finite()
            || logical_size.min_element() <= 0.0
        {
            return Err(HandleError::InvalidCapture);
        }
        let aspect = physical_size[0] as f32 / physical_size[1] as f32;
        let projection = camera.projection_matrix(aspect)?.as_dmat4();
        let view_projection = projection * camera.view_matrix().as_dmat4();
        let focal_pixels = projection.y_axis.y * f64::from(physical_size[1]) * 0.5;
        if !view_projection.is_finite() || !focal_pixels.is_finite() || focal_pixels <= 0.0 {
            return Err(HandleError::InvalidCapture);
        }
        Ok(Self {
            camera: camera.clone(),
            physical_origin,
            physical_size,
            pixels_per_point,
            logical_min,
            logical_size,
            aspect,
            view_projection,
            focal_pixels,
            handle_length: ARROW_LENGTH,
        })
    }

    /// The drawn length is `max(72 * size, 36)` pixels, before the view foreshortening.
    pub fn with_manipulator_size(mut self, size: ManipulatorSize) -> Self {
        self.handle_length = (ARROW_LENGTH * size.get()).max(MIN_HANDLE_LENGTH);
        self
    }

    pub(crate) fn handle_length(&self) -> f64 {
        self.handle_length
    }

    pub fn same_camera(&self, camera: &Camera) -> bool {
        self.camera.same_capture(camera)
    }

    pub fn same_mapping(&self, other: &Self) -> bool {
        self.camera.same_capture(&other.camera)
            && self.physical_origin == other.physical_origin
            && self.physical_size == other.physical_size
            && self.pixels_per_point.to_bits() == other.pixels_per_point.to_bits()
            && self.logical_min.to_array().map(f32::to_bits)
                == other.logical_min.to_array().map(f32::to_bits)
            && self.logical_size.to_array().map(f32::to_bits)
                == other.logical_size.to_array().map(f32::to_bits)
            && self.aspect.to_bits() == other.aspect.to_bits()
            && self.view_projection.to_cols_array().map(f64::to_bits)
                == other.view_projection.to_cols_array().map(f64::to_bits)
    }

    pub fn pointer_pixel(&self, pointer: Vec2) -> Result<DVec2, HandleError> {
        if !pointer.is_finite() {
            return Err(HandleError::NonFinitePointer);
        }
        let pixel = (pointer - self.logical_min) * self.pixels_per_point;
        if !pixel.is_finite() {
            return Err(HandleError::NonFinitePointer);
        }
        Ok(pixel.as_dvec2())
    }

    pub(crate) fn ray_at_pixel(&self, pixel: DVec2) -> Result<Ray, HandleError> {
        let size = self.size();
        let ndc = Vec2::new(
            (2.0 * pixel.x / size.x - 1.0) as f32,
            (1.0 - 2.0 * pixel.y / size.y) as f32,
        );
        Ok(self.camera.captured_ray(ndc, self.aspect)?)
    }

    pub(crate) fn size(&self) -> DVec2 {
        DVec2::new(
            f64::from(self.physical_size[0]),
            f64::from(self.physical_size[1]),
        )
    }

    pub(crate) fn contains(&self, pixel: DVec2) -> bool {
        pixel.is_finite() && pixel.cmpge(DVec2::ZERO).all() && pixel.cmple(self.size()).all()
    }

    pub(crate) fn eye(&self) -> DVec3 {
        self.camera.eye().as_dvec3()
    }

    /// Viewport pixel of a point in front of the camera; it may lie outside the viewport.
    pub(crate) fn project_point(&self, point: DVec3) -> Option<DVec2> {
        let clip = self.view_projection * point.extend(1.0);
        if !clip.is_finite() || clip.w <= 1e-9 {
            return None;
        }
        let size = self.size();
        let pixel = DVec2::new(
            (clip.x / clip.w + 1.0) * size.x * 0.5,
            (1.0 - clip.y / clip.w) * size.y * 0.5,
        );
        pixel.is_finite().then_some(pixel)
    }

    /// World length that spans `pixels` on screen at the depth `clip_w`.
    pub(crate) fn world_length(&self, pixels: f64, clip_w: f64) -> f64 {
        pixels * clip_w / self.focal_pixels
    }

    /// Clip-space position and viewport pixel of a handle origin, which must be visible.
    pub(crate) fn project_origin(&self, origin_cm: DVec3) -> Result<(DVec4, DVec2), HandleError> {
        if !origin_cm.is_finite() {
            return Err(HandleError::InvalidProjection);
        }
        let clip = self.view_projection * origin_cm.extend(1.0);
        if !clip.is_finite() {
            return Err(HandleError::InvalidProjection);
        }
        if clip.w <= 0.0 || clip.z < 0.0 || clip.z > clip.w {
            return Err(HandleError::OriginClipped);
        }
        let ndc = clip.truncate() / clip.w;
        let size = self.size();
        let pixel = DVec2::new((ndc.x + 1.0) * size.x * 0.5, (1.0 - ndc.y) * size.y * 0.5);
        if !self.contains(pixel) {
            return Err(HandleError::OriginClipped);
        }
        Ok((clip, pixel))
    }

    /// Fraction of a world axis that shows on screen: the sine of its angle to the view ray
    /// through `origin_cm`. One when perpendicular to the view, zero when seen end-on.
    /// A first-order approximation: exact end-on. Relative to an on-axis handle at the same
    /// distance, an off-centre one (view ray `a` off the camera axis) shows longer on screen by
    /// 1/cos(a) to 1/cos^2(a), so this rule is short by `1 - cos(a)` to `1 - cos^2(a)` (29-50%
    /// at the edge midpoints, 42-67% in the corners of a square viewport at 90 degrees).
    pub(crate) fn projected_fraction(
        &self,
        origin_cm: DVec3,
        direction: DVec3,
    ) -> Result<f64, HandleError> {
        let to_origin = origin_cm - self.eye();
        let length = to_origin.length();
        if !length.is_finite() || length <= 0.0 {
            return Err(HandleError::InvalidProjection);
        }
        let cosine = direction.dot(to_origin / length);
        let fraction = (1.0 - cosine * cosine).max(0.0).sqrt();
        if !fraction.is_finite() {
            return Err(HandleError::InvalidProjection);
        }
        Ok(fraction)
    }

    /// Screen vector of a handle axis: its unit screen direction scaled to the handle length times
    /// the projected fraction, so an axis seen end-on shrinks instead of keeping full length.
    pub(crate) fn axis_screen_vector(
        &self,
        origin_cm: DVec3,
        clip: DVec4,
        direction: DVec3,
    ) -> Result<(DVec2, f64), HandleError> {
        let unit = self.screen_direction(clip, direction)?;
        let length = self.handle_length * self.projected_fraction(origin_cm, direction)?;
        Ok((unit, length))
    }

    /// Unit screen direction of a world direction at a projected origin.
    pub(crate) fn screen_direction(
        &self,
        clip: DVec4,
        direction: DVec3,
    ) -> Result<DVec2, HandleError> {
        let size = self.size();
        let projected = self.view_projection * direction.extend(0.0);
        let denominator = clip.w * clip.w;
        if !denominator.is_finite() || denominator == 0.0 {
            return Err(HandleError::InvalidProjection);
        }
        let derivative = DVec2::new(
            (projected.x * clip.w - clip.x * projected.w) / denominator * size.x * 0.5,
            -(projected.y * clip.w - clip.y * projected.w) / denominator * size.y * 0.5,
        );
        let length = derivative.length();
        if !derivative.is_finite() || !length.is_finite() || length == 0.0 {
            return Err(HandleError::ProjectedAxisDegenerate);
        }
        Ok(derivative / length)
    }
}

#[derive(Clone, Copy)]
pub struct ScreenSegment {
    pub start: DVec2,
    pub end: DVec2,
}

pub struct ScreenArrow {
    segments: [Option<ScreenSegment>; 4],
    label: DVec2,
}

impl ScreenArrow {
    pub fn segments(&self) -> impl Iterator<Item = &ScreenSegment> {
        self.segments.iter().flatten()
    }
    pub fn label(&self) -> DVec2 {
        self.label
    }
}

pub struct AxisHandle {
    pub axis: WorldAxis,
    geometry: Result<ScreenArrow, HandleError>,
}

impl AxisHandle {
    pub fn arrow(&self) -> Option<&ScreenArrow> {
        self.geometry.as_ref().ok()
    }
    pub fn unavailable_reason(&self) -> Option<HandleError> {
        self.geometry.as_ref().err().copied()
    }
}

pub struct WorldHandles {
    origin: DVec2,
    size: DVec2,
    axes: [AxisHandle; 3],
}

impl WorldHandles {
    pub fn new(capture: &ViewportCapture, origin_cm: DVec3) -> Result<Self, HandleError> {
        let (clip, origin) = capture.project_origin(origin_cm)?;
        let size = capture.size();
        let axes = [WorldAxis::X, WorldAxis::Y, WorldAxis::Z].map(|axis| {
            let geometry = (|| {
                axis_parameter(origin_cm, axis, capture.ray_at_pixel(origin)?)?;
                let (direction, length) =
                    capture.axis_screen_vector(origin_cm, clip, axis.direction())?;
                let tip = origin + direction * length;
                let head = length.min(ARROWHEAD_LENGTH);
                let base = tip - direction * head;
                let perpendicular = DVec2::new(-direction.y, direction.x)
                    * (ARROWHEAD_HALF_WIDTH * head / ARROWHEAD_LENGTH);
                let left = base + perpendicular;
                let right = base - perpendicular;
                let segments = [(origin, tip), (tip, left), (left, right), (right, tip)]
                    .map(|(start, end)| clip_segment(start, end, size));
                if !segments
                    .iter()
                    .flatten()
                    .any(|segment| segment.start != segment.end)
                {
                    return Err(HandleError::HandleClipped);
                }
                Ok(ScreenArrow {
                    segments,
                    label: (tip + direction * 10.0).clamp(DVec2::ZERO, size),
                })
            })();
            AxisHandle { axis, geometry }
        });
        Ok(Self { origin, size, axes })
    }

    pub fn axes(&self) -> &[AxisHandle; 3] {
        &self.axes
    }

    pub fn hit_test(&self, pixel: DVec2) -> Option<WorldAxis> {
        if !pixel.is_finite()
            || pixel.cmplt(DVec2::ZERO).any()
            || pixel.cmpgt(self.size).any()
            || pixel.distance_squared(self.origin) <= CENTER_EXCLUSION * CENTER_EXCLUSION
        {
            return None;
        }
        let mut closest = HIT_RADIUS * HIT_RADIUS;
        let mut selected = None;
        for handle in &self.axes {
            if let Ok(arrow) = &handle.geometry {
                for segment in arrow.segments() {
                    let distance = segment_distance_squared(*segment, pixel);
                    if distance <= HIT_RADIUS * HIT_RADIUS
                        && (selected.is_none() || distance < closest)
                    {
                        closest = distance;
                        selected = Some(handle.axis);
                    }
                }
            }
        }
        selected
    }
}

pub struct AxisDrag {
    capture: ViewportCapture,
    origin_cm: DVec3,
    axis: WorldAxis,
    begin_parameter: f64,
}

impl AxisDrag {
    pub fn begin(
        capture: ViewportCapture,
        origin_cm: DVec3,
        axis: WorldAxis,
        pointer: Vec2,
    ) -> Result<Self, HandleError> {
        let pixel = capture.pointer_pixel(pointer)?;
        if !capture.contains(pixel) {
            return Err(HandleError::OriginClipped);
        }
        let begin_parameter = axis_parameter(origin_cm, axis, capture.ray_at_pixel(pixel)?)?;
        Ok(Self {
            capture,
            origin_cm,
            axis,
            begin_parameter,
        })
    }

    pub fn same_capture(&self, current_capture: &ViewportCapture) -> bool {
        self.capture.same_mapping(current_capture)
    }

    pub fn distance_cm(
        &self,
        current_capture: &ViewportCapture,
        pointer: Vec2,
    ) -> Result<f64, HandleError> {
        if !self.capture.same_mapping(current_capture) {
            return Err(HandleError::CaptureChanged);
        }
        let pixel = self.capture.pointer_pixel(pointer)?;
        let parameter =
            axis_parameter(self.origin_cm, self.axis, self.capture.ray_at_pixel(pixel)?)?;
        let distance = parameter - self.begin_parameter;
        if !distance.is_finite() {
            return Err(HandleError::InvalidSolve);
        }
        Ok(distance)
    }
}

pub(crate) fn segment_distance_squared(segment: ScreenSegment, point: DVec2) -> f64 {
    let delta = segment.end - segment.start;
    let length_squared = delta.length_squared();
    if length_squared == 0.0 {
        return point.distance_squared(segment.start);
    }
    let parameter = ((point - segment.start).dot(delta) / length_squared).clamp(0.0, 1.0);
    point.distance_squared(segment.start + delta * parameter)
}

pub(crate) fn clip_segment(start: DVec2, end: DVec2, size: DVec2) -> Option<ScreenSegment> {
    let delta = end - start;
    let mut begin: f64 = 0.0;
    let mut finish: f64 = 1.0;
    for (p, q) in [
        (-delta.x, start.x),
        (delta.x, size.x - start.x),
        (-delta.y, start.y),
        (delta.y, size.y - start.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return None;
            }
        } else {
            let parameter = q / p;
            if p < 0.0 {
                begin = begin.max(parameter);
            } else {
                finish = finish.min(parameter);
            }
            if begin > finish {
                return None;
            }
        }
    }
    Some(ScreenSegment {
        start: (start + delta * begin).clamp(DVec2::ZERO, size),
        end: (start + delta * finish).clamp(DVec2::ZERO, size),
    })
}

fn axis_parameter(origin_cm: DVec3, axis: WorldAxis, ray: Ray) -> Result<f64, HandleError> {
    axis_parameter_along(origin_cm, axis.direction(), ray)
}

/// Signed distance from `origin_cm` along the unit `axis` to the point closest to `ray`.
pub(crate) fn axis_parameter_along(
    origin_cm: DVec3,
    axis: DVec3,
    ray: Ray,
) -> Result<f64, HandleError> {
    if !origin_cm.is_finite() || !ray.origin.is_finite() || !ray.direction.is_finite() {
        return Err(HandleError::InvalidSolve);
    }
    let direction = ray.direction.as_dvec3();
    let length = direction.length();
    if !length.is_finite() || length <= 0.0 {
        return Err(HandleError::InvalidSolve);
    }
    let direction = direction / length;
    let dot = axis.dot(direction);
    let denominator = 1.0 - dot * dot;
    if !denominator.is_finite() || denominator <= PARALLEL_LIMIT {
        return Err(HandleError::ParallelAxis);
    }
    let offset = origin_cm - ray.origin.as_dvec3();
    let parameter = (dot * direction.dot(offset) - axis.dot(offset)) / denominator;
    let ray_parameter = direction.dot(offset) + dot * parameter;
    if !parameter.is_finite() || !ray_parameter.is_finite() {
        return Err(HandleError::InvalidSolve);
    }
    if ray_parameter < 0.0 {
        return Err(HandleError::BehindRay);
    }
    Ok(parameter)
}

#[cfg(test)]
mod tests;
