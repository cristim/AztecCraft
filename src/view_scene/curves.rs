use super::{ViewError, buffer_bytes, checked_add, reserve};
use crate::native_scene::NativeCurveShape;
use glam::{DMat4, DVec3, Vec3};
use std::mem::size_of;

const MAX_SEGMENTS: usize = 65_536;
const MAX_DEPTH: usize = 24;
const MAX_WORK: usize = 1_000_000;
/// Largest f32 grid at a curve's coordinates, as a fraction of its size, that the f32 camera can
/// still frame and that overlays still agree with the curve within.
const CAMERA_GRID_FRACTION: f64 = 0.05;
/// Coordinate beyond which f32 distance-squared overflows; a coarse backstop for single-point
/// curves, which the grid gate exempts (the camera may still refuse them below it).
const MAX_CAMERA_COORDINATE: f64 = 1e18;

#[derive(Default)]
pub(super) struct CurveWork(usize);

impl CurveWork {
    fn charge(&mut self) -> Result<(), ViewError> {
        self.0 = checked_add(self.0, 1)?;
        if self.0 > MAX_WORK {
            return Err(ViewError::ResourceLimit);
        }
        Ok(())
    }
}

pub(super) struct PreparedCurve {
    /// f64 world point the f32 `segments` are offsets from, so far-from-origin curves keep
    /// their precision.
    pub(super) origin: DVec3,
    pub(super) segments: Vec<[Vec3; 2]>,
    pub(super) bounds: [DVec3; 2],
}

#[derive(Clone, Copy)]
struct Piece {
    controls: [DVec3; 4],
    depth: usize,
}

pub(super) fn prepare(
    curve: &NativeCurveShape,
    world: &DMat4,
    byte_budget: usize,
    work: &mut CurveWork,
) -> Result<PreparedCurve, ViewError> {
    let fixed = checked_add(
        size_of::<PreparedCurve>(),
        size_of::<[Option<Piece>; MAX_DEPTH + 1]>(),
    )?;
    if fixed > byte_budget {
        return Err(ViewError::ResourceLimit);
    }
    let mut bounds = [DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)];
    for cv in curve.control_vertices() {
        work.charge()?;
        let point = world_point(world, DVec3::from_array(*cv))?;
        bounds[0] = bounds[0].min(point);
        bounds[1] = bounds[1].max(point);
    }
    let diagonal = scalar(squared(point(bounds[1] - bounds[0])?)?.sqrt())?;
    let magnitude = bounds[0].abs().max(bounds[1].abs()).max_element();
    // Segments are f32 offsets from an f64 origin, but the camera target and eye, and every
    // non-curve overlay, are still world-space f32. Past a grid of CAMERA_GRID_FRACTION of the
    // curve the camera cannot frame or orbit it and overlays drift from it by more than that,
    // so the curve is refused loudly instead of drawn off-screen. A single-point curve has no
    // shape to distort.
    if magnitude > MAX_CAMERA_COORDINATE {
        return Err(ViewError::NonFiniteWorld);
    }
    let camera_grid = scalar(magnitude * f64::from(f32::EPSILON))?;
    if diagonal > 0.0 && camera_grid > scalar(diagonal * CAMERA_GRID_FRACTION)? {
        return Err(ViewError::NonFiniteWorld);
    }
    let grid = scalar(magnitude * f64::EPSILON)?;
    let origin = point(point(bounds[0] * 0.5)? + point(bounds[1] * 0.5)?)?;
    // f32 endpoint rounding of an offset costs up to diagonal * f32::EPSILON (covering the
    // 3-axis norm of the half-diagonal) on top of the f64 grid.
    let representable = scalar(diagonal * f64::from(f32::EPSILON))? + grid;
    let tolerance = scalar(diagonal * 1e-4)?.max(1e-6) + representable;
    let tolerance_squared = scalar(tolerance * tolerance)?;
    let count = walk(curve, world, origin, tolerance_squared, work, |_| Ok(()))?;
    if checked_add(fixed, buffer_bytes::<[Vec3; 2]>(count)?)? > byte_budget {
        return Err(ViewError::ResourceLimit);
    }
    let mut segments = reserve(count)?;
    if checked_add(fixed, buffer_bytes::<[Vec3; 2]>(segments.capacity())?)? > byte_budget {
        return Err(ViewError::ResourceLimit);
    }
    let actual = walk(curve, world, origin, tolerance_squared, work, |segment| {
        if segments.len() >= count {
            return Err(ViewError::InvalidGeometry);
        }
        segments.push(segment);
        Ok(())
    })?;
    if actual != count {
        return Err(ViewError::InvalidGeometry);
    }
    Ok(PreparedCurve {
        origin,
        segments,
        bounds,
    })
}

fn walk(
    curve: &NativeCurveShape,
    world: &DMat4,
    origin: DVec3,
    tolerance_squared: f64,
    work: &mut CurveWork,
    mut emit: impl FnMut([Vec3; 2]) -> Result<(), ViewError>,
) -> Result<usize, ViewError> {
    let spans = curve
        .control_vertices()
        .len()
        .checked_sub(curve.degree() as usize)
        .ok_or(ViewError::InvalidGeometry)?;
    let mut count = 0usize;
    let mut stack = [None; MAX_DEPTH + 1];
    for span in 0..spans {
        work.charge()?;
        let mut controls = curve.bezier_span(span).map_err(ViewError::Native)?;
        for control in &mut controls {
            *control = point(world_point(world, *control)? - origin)?;
        }
        stack[0] = Some(Piece { controls, depth: 0 });
        let mut length = 1usize;
        while length > 0 {
            work.charge()?;
            length -= 1;
            let piece = stack[length].take().ok_or(ViewError::InvalidGeometry)?;
            let endpoints = [quantize(piece.controls[0])?, quantize(piece.controls[3])?];
            let a = endpoints[0].as_dvec3();
            let b = endpoints[1].as_dvec3();
            if squared(point(piece.controls[0] - a)?)? > tolerance_squared
                || squared(point(piece.controls[3] - b)?)? > tolerance_squared
            {
                return Err(ViewError::NonFiniteWorld);
            }
            let mut flat = true;
            for control in piece.controls {
                if distance_squared(control, a, b)? > tolerance_squared {
                    flat = false;
                }
            }
            if flat {
                count = checked_add(count, 1)?;
                if count > MAX_SEGMENTS {
                    return Err(ViewError::ResourceLimit);
                }
                emit(endpoints)?;
            } else {
                if piece.depth >= MAX_DEPTH || length + 2 > stack.len() {
                    return Err(ViewError::ResourceLimit);
                }
                let [left, right] = split(piece.controls)?;
                stack[length] = Some(Piece {
                    controls: right,
                    depth: piece.depth + 1,
                });
                stack[length + 1] = Some(Piece {
                    controls: left,
                    depth: piece.depth + 1,
                });
                length += 2;
            }
        }
    }
    Ok(count)
}

fn split(p: [DVec3; 4]) -> Result<[[DVec3; 4]; 2], ViewError> {
    let a = midpoint(p[0], p[1])?;
    let b = midpoint(p[1], p[2])?;
    let c = midpoint(p[2], p[3])?;
    let d = midpoint(a, b)?;
    let e = midpoint(b, c)?;
    let middle = midpoint(d, e)?;
    Ok([[p[0], a, d, middle], [middle, e, c, p[3]]])
}

fn midpoint(a: DVec3, b: DVec3) -> Result<DVec3, ViewError> {
    point(point(a * 0.5)? + point(b * 0.5)?)
}

fn distance_squared(p: DVec3, a: DVec3, b: DVec3) -> Result<f64, ViewError> {
    let v = point(b - a)?;
    let r = point(p - a)?;
    if v == DVec3::ZERO {
        return squared(r);
    }
    let denominator = squared(v)?;
    if denominator <= 0.0 {
        return Err(ViewError::NonFiniteWorld);
    }
    let projection = scalar(dot(r, v)? / denominator)?.clamp(0.0, 1.0);
    squared(point(r - point(v * projection)?)?)
}

fn world_point(world: &DMat4, p: DVec3) -> Result<DVec3, ViewError> {
    if !world.is_finite()
        || world.x_axis.w != 0.0
        || world.y_axis.w != 0.0
        || world.z_axis.w != 0.0
        || world.w_axis.w != 1.0
    {
        return Err(ViewError::NonFiniteWorld);
    }
    let x = point(world.x_axis.truncate() * p.x)?;
    let y = point(world.y_axis.truncate() * p.y)?;
    let z = point(world.z_axis.truncate() * p.z)?;
    point(point(point(x + y)? + z)? + world.w_axis.truncate())
}

fn quantize(value: DVec3) -> Result<Vec3, ViewError> {
    let value = value.as_vec3();
    value
        .is_finite()
        .then_some(value)
        .ok_or(ViewError::NonFiniteWorld)
}

fn dot(a: DVec3, b: DVec3) -> Result<f64, ViewError> {
    scalar(scalar(scalar(a.x * b.x)? + scalar(a.y * b.y)?)? + scalar(a.z * b.z)?)
}

fn squared(value: DVec3) -> Result<f64, ViewError> {
    dot(value, value)
}

fn scalar(value: f64) -> Result<f64, ViewError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(ViewError::NonFiniteWorld)
}

fn point(value: DVec3) -> Result<DVec3, ViewError> {
    value
        .is_finite()
        .then_some(value)
        .ok_or(ViewError::NonFiniteWorld)
}

#[cfg(test)]
mod tests;
