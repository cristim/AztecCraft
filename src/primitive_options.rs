//! Validated creation options for polygon primitives.
//!
//! The limits are AztecCraft safety bounds (PROVISIONAL, #5): the Maya documentation states no
//! numeric ranges. Sizes are in internal centimetre units.

use crate::scene::Primitive;
use std::fmt;

pub const MIN_SIZE_CM: f64 = 1e-6;
pub const MAX_SIZE_CM: f64 = 1e6;
// Replaces a per-build f32 check: every size in range is a normal, finite f32.
const _: () =
    assert!(((MIN_SIZE_CM / 256.0) as f32).is_normal() && (MAX_SIZE_CM as f32).is_finite());
pub const MIN_DIVISIONS: u32 = 1;
pub const MIN_AXIS_DIVISIONS: u32 = 3;
pub const MIN_SPHERE_HEIGHT_DIVISIONS: u32 = 2;
pub const MAX_DIVISIONS: u32 = 128;
/// A 128x128 grid builds in about 40 ms unoptimised; 20,000 leaves headroom for per-edit
/// revalidation and display. It must stay below the 128^3 cube's 98,304 faces or the budget
/// can never reject anything.
pub const MAX_FACES: u64 = 20_000;

pub const DEFAULT_AXIS: Axis = Axis::Y;
pub const DEFAULT_CUBE_SIZE_CM: f64 = 1.0;
pub const DEFAULT_CUBE_DIVISIONS: u32 = 1;
pub const DEFAULT_PLANE_SIZE_CM: f64 = 1.0;
pub const DEFAULT_PLANE_DIVISIONS: u32 = 1;
pub const DEFAULT_RADIUS_CM: f64 = 1.0;
/// Cylinder and cone height.
pub const DEFAULT_HEIGHT_CM: f64 = 2.0;
pub const DEFAULT_AXIS_DIVISIONS: u32 = 20;
pub const DEFAULT_SPHERE_HEIGHT_DIVISIONS: u32 = 20;
pub const DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS: u32 = 1;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveOptionsError {
    PrimitiveMismatch {
        expected: Primitive,
    },
    SizeOutOfRange {
        field: &'static str,
    },
    DivisionsOutOfRange {
        field: &'static str,
        min: u32,
        max: u32,
    },
    TooManyFaces {
        faces: u64,
        budget: u64,
    },
}

impl fmt::Display for PrimitiveOptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PrimitiveMismatch { expected } => {
                write!(f, "options do not belong to a {expected:?}")
            }
            Self::SizeOutOfRange { field } => write!(
                f,
                "{field} must be finite and between {MIN_SIZE_CM} and {MAX_SIZE_CM}"
            ),
            Self::DivisionsOutOfRange { field, min, max } => {
                write!(f, "{field} must be a whole number from {min} to {max}")
            }
            Self::TooManyFaces { faces, budget } => {
                write!(f, "{faces} faces exceed the limit of {budget}")
            }
        }
    }
}

impl std::error::Error for PrimitiveOptionsError {}

impl PrimitiveOptions {
    // PROVISIONAL (#5): sphere radius 1 and 20x20 are the polySphere node defaults; cylinder and cone
    // radius 1, height 2 and 20x1 follow the polyCylinder (sc=0 variant) and polyCone node pages
    // (the command page lists sz 1); Maya 2026 is unverified.
    /// Options equal to the meshes created before options existed.
    pub fn default_for(primitive: Primitive) -> Self {
        let axis = DEFAULT_AXIS;
        match primitive {
            Primitive::Cube => Self::Cube {
                width: DEFAULT_CUBE_SIZE_CM,
                height: DEFAULT_CUBE_SIZE_CM,
                depth: DEFAULT_CUBE_SIZE_CM,
                sub_width: DEFAULT_CUBE_DIVISIONS,
                sub_height: DEFAULT_CUBE_DIVISIONS,
                sub_depth: DEFAULT_CUBE_DIVISIONS,
                axis,
            },
            Primitive::Plane => Self::Plane {
                width: DEFAULT_PLANE_SIZE_CM,
                height: DEFAULT_PLANE_SIZE_CM,
                sub_width: DEFAULT_PLANE_DIVISIONS,
                sub_height: DEFAULT_PLANE_DIVISIONS,
                axis,
            },
            Primitive::Sphere => Self::Sphere {
                radius: DEFAULT_RADIUS_CM,
                sub_axis: DEFAULT_AXIS_DIVISIONS,
                sub_height: DEFAULT_SPHERE_HEIGHT_DIVISIONS,
                axis,
            },
            Primitive::Cylinder => Self::Cylinder {
                radius: DEFAULT_RADIUS_CM,
                height: DEFAULT_HEIGHT_CM,
                sub_axis: DEFAULT_AXIS_DIVISIONS,
                sub_height: DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS,
                axis,
            },
            Primitive::Cone => Self::Cone {
                radius: DEFAULT_RADIUS_CM,
                height: DEFAULT_HEIGHT_CM,
                sub_axis: DEFAULT_AXIS_DIVISIONS,
                sub_height: DEFAULT_CYLINDER_CONE_HEIGHT_DIVISIONS,
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

    /// Checks, in order: variant, field ranges, then the checked face count against the budget.
    /// Nothing is allocated for the mesh.
    pub fn validate(&self, expected: Primitive) -> Result<(), PrimitiveOptionsError> {
        if self.primitive() != expected {
            return Err(PrimitiveOptionsError::PrimitiveMismatch { expected });
        }
        let (sizes, divisions) = self.fields();
        for (field, value) in &sizes {
            check_size(field, *value)?;
        }
        for (field, value, min) in &divisions {
            if !(*min..=MAX_DIVISIONS).contains(value) {
                return Err(PrimitiveOptionsError::DivisionsOutOfRange {
                    field,
                    min: *min,
                    max: MAX_DIVISIONS,
                });
            }
        }
        let faces = self.face_count();
        if faces > MAX_FACES {
            return Err(PrimitiveOptionsError::TooManyFaces {
                faces,
                budget: MAX_FACES,
            });
        }
        Ok(())
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

    fn fields(&self) -> (Vec<Size>, Vec<Division>) {
        match *self {
            Self::Cube {
                width,
                height,
                depth,
                sub_width,
                sub_height,
                sub_depth,
                ..
            } => (
                vec![("width", width), ("height", height), ("depth", depth)],
                vec![
                    ("subdivisions_width", sub_width, MIN_DIVISIONS),
                    ("subdivisions_height", sub_height, MIN_DIVISIONS),
                    ("subdivisions_depth", sub_depth, MIN_DIVISIONS),
                ],
            ),
            Self::Plane {
                width,
                height,
                sub_width,
                sub_height,
                ..
            } => (
                vec![("width", width), ("height", height)],
                vec![
                    ("subdivisions_width", sub_width, MIN_DIVISIONS),
                    ("subdivisions_height", sub_height, MIN_DIVISIONS),
                ],
            ),
            Self::Sphere {
                radius,
                sub_axis,
                sub_height,
                ..
            } => (
                vec![("radius", radius)],
                vec![
                    ("subdivisions_axis", sub_axis, MIN_AXIS_DIVISIONS),
                    (
                        "subdivisions_height",
                        sub_height,
                        MIN_SPHERE_HEIGHT_DIVISIONS,
                    ),
                ],
            ),
            Self::Cylinder {
                radius,
                height,
                sub_axis,
                sub_height,
                ..
            }
            | Self::Cone {
                radius,
                height,
                sub_axis,
                sub_height,
                ..
            } => (
                vec![("radius", radius), ("height", height)],
                vec![
                    ("subdivisions_axis", sub_axis, MIN_AXIS_DIVISIONS),
                    ("subdivisions_height", sub_height, MIN_DIVISIONS),
                ],
            ),
        }
    }
}

/// Field name and value.
type Size = (&'static str, f64);
/// Field name, value and minimum.
type Division = (&'static str, u32, u32);

fn check_size(field: &'static str, value: f64) -> Result<(), PrimitiveOptionsError> {
    if (MIN_SIZE_CM..=MAX_SIZE_CM).contains(&value) {
        Ok(())
    } else {
        Err(PrimitiveOptionsError::SizeOutOfRange { field })
    }
}

#[cfg(test)]
mod tests;
