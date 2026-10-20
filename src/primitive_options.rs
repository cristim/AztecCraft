//! Validated creation options for polygon primitives.
//!
//! The limits are AztecCraft safety bounds (PROVISIONAL, #5): the Maya documentation states no
//! numeric ranges. Sizes are in internal centimetre units.

use crate::scene::Primitive;

pub const MIN_SIZE_CM: f64 = 1e-6;
pub const MAX_SIZE_CM: f64 = 1e6;
pub const MIN_DIVISIONS: u32 = 1;
pub const MIN_AXIS_DIVISIONS: u32 = 3;
pub const MIN_SPHERE_HEIGHT_DIVISIONS: u32 = 2;
pub const MAX_DIVISIONS: u32 = 128;
/// Chosen from build timings: a 65,536-face grid builds in about 150 ms unoptimised, so 20,000
/// leaves headroom for per-edit revalidation and display. It must stay below the 128^3 cube's
/// 98,304 faces or the budget can never reject anything.
pub const MAX_FACES: u64 = 20_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Axis {
    X,
    #[default]
    Y,
    Z,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrimitiveOptions {
    Cube {
        width: f64,
        height: f64,
        depth: f64,
        sub_width: u32,
        sub_height: u32,
        sub_depth: u32,
        axis: Axis,
    },
    Plane {
        width: f64,
        height: f64,
        sub_width: u32,
        sub_height: u32,
        axis: Axis,
    },
    Sphere {
        radius: f64,
        sub_axis: u32,
        sub_height: u32,
        axis: Axis,
    },
    Cylinder {
        radius: f64,
        height: f64,
        sub_axis: u32,
        sub_height: u32,
        axis: Axis,
    },
    Cone {
        radius: f64,
        height: f64,
        sub_axis: u32,
        sub_height: u32,
        axis: Axis,
    },
}

impl PrimitiveOptions {
    /// Options equal to the meshes created before options existed.
    pub fn default_for(primitive: Primitive) -> Self {
        let axis = Axis::Y;
        match primitive {
            Primitive::Cube => Self::Cube {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
                sub_width: 1,
                sub_height: 1,
                sub_depth: 1,
                axis,
            },
            Primitive::Plane => Self::Plane {
                width: 1.0,
                height: 1.0,
                sub_width: 1,
                sub_height: 1,
                axis,
            },
            Primitive::Sphere => Self::Sphere {
                radius: 1.0,
                sub_axis: 20,
                sub_height: 20,
                axis,
            },
            Primitive::Cylinder => Self::Cylinder {
                radius: 1.0,
                height: 2.0,
                sub_axis: 20,
                sub_height: 1,
                axis,
            },
            Primitive::Cone => Self::Cone {
                radius: 1.0,
                height: 2.0,
                sub_axis: 20,
                sub_height: 1,
                axis,
            },
        }
    }

    pub fn primitive(&self) -> Primitive {
        match self {
            Self::Cube { .. } => Primitive::Cube,
            Self::Plane { .. } => Primitive::Plane,
            Self::Sphere { .. } => Primitive::Sphere,
            Self::Cylinder { .. } => Primitive::Cylinder,
            Self::Cone { .. } => Primitive::Cone,
        }
    }

    /// Faces of the mesh these options build, including caps; saturates instead of wrapping.
    pub fn face_count(&self) -> u64 {
        let counted = match *self {
            Self::Cube {
                sub_width: w,
                sub_height: h,
                sub_depth: d,
                ..
            } => {
                let (w, h, d) = (u64::from(w), u64::from(h), u64::from(d));
                w.checked_mul(h)
                    .zip(h.checked_mul(d))
                    .zip(w.checked_mul(d))
                    .and_then(|((a, b), c)| a.checked_add(b)?.checked_add(c))
                    .and_then(|sum| sum.checked_mul(2))
            }
            Self::Plane {
                sub_width,
                sub_height,
                ..
            } => u64::from(sub_width).checked_mul(u64::from(sub_height)),
            Self::Sphere {
                sub_axis,
                sub_height,
                ..
            } => u64::from(sub_axis).checked_mul(u64::from(sub_height)),
            Self::Cylinder {
                sub_axis,
                sub_height,
                ..
            } => u64::from(sub_axis)
                .checked_mul(u64::from(sub_height))
                .and_then(|n| n.checked_add(2)),
            Self::Cone {
                sub_axis,
                sub_height,
                ..
            } => u64::from(sub_axis)
                .checked_mul(u64::from(sub_height))
                .and_then(|n| n.checked_add(1)),
        };
        counted.unwrap_or(u64::MAX)
    }
}

#[cfg(test)]
mod tests;
