use crate::camera_optics::CameraOptics;
// The camera lens: an explicit vertical field of view, or the film back of Maya's default camera.

/// Create Camera Options, Focal Length: "The default value is 35." Millimetres.
pub const FOCAL_MM: f32 = 35.0;
/// Create Camera Options, Film Aperture: "The default values are 1.4173 and 0.9449." Inches,
/// horizontal first.
pub const APERTURE_IN: [f32; 2] = [1.4173, 0.9449];
const MM_PER_INCH: f32 = 25.4;

/// `Film` is the Fill film fit of the default film back (PROVISIONAL, see row 25 of
/// `.project-docs/development.md`); Horizontal, Vertical and Overscan are unsupported.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lens {
    /// Vertical field of view in radians, whatever the viewport aspect.
    VerticalFov(f32),
    /// Focal length and aperture above, fitted to the viewport as the resolution gate.
    Film,
    /// Immutable scene-derived optical snapshot.
    Optics(CameraOptics),
    /// Source contributors prevent faithful projection.
    Unsupported,
}

/// Pinhole geometry: tan(half angle) = aperture * 25.4 / (2 * focal). The docs do not state it
/// (PROVISIONAL).
fn film_tan_half(aperture_in: f32) -> f32 {
    aperture_in * MM_PER_INCH / (2.0 * FOCAL_MM)
}

impl Lens {
    /// Half the vertical field of view in radians at the given viewport aspect. Fill fits the
    /// resolution gate within the film gate: a viewport at least as wide as the film matches
    /// horizontally, a narrower one vertically.
    pub(super) fn half_angle_y(self, aspect: f32) -> f32 {
        match self {
            Self::VerticalFov(fov_y) => fov_y * 0.5,
            _ => self.tan_half_y(aspect).atan(),
        }
    }

    /// Tangent of `half_angle_y`; the projection scale is its reciprocal.
    pub(super) fn tan_half_y(self, aspect: f32) -> f32 {
        match self {
            Self::VerticalFov(fov_y) => (fov_y * 0.5).tan(),
            Self::Optics(o) => o
                .frustum(f64::from(aspect), 1.0)
                .map(|v| ((v[3] - v[2]) * 0.5) as f32)
                .unwrap_or(f32::NAN),
            Self::Unsupported => f32::NAN,
            Self::Film => {
                let [horizontal, vertical] = APERTURE_IN;
                if aspect >= horizontal / vertical {
                    film_tan_half(horizontal) / aspect
                } else {
                    film_tan_half(vertical)
                }
            }
        }
    }

    pub(super) fn is_valid(self) -> bool {
        match self {
            Self::VerticalFov(fov_y) => {
                fov_y.is_finite() && fov_y > 0.0 && fov_y < std::f32::consts::PI
            }
            Self::Film => true,
            Self::Optics(o) => o.validate().is_ok(),
            Self::Unsupported => false,
        }
    }

    /// Discriminant and angle bits, for the bit-exact `Camera::same_capture`.
    pub(super) fn capture_bits(self) -> [u64; 19] {
        let mut bits = [0; 19];
        match self {
            Self::VerticalFov(v) => bits[1] = u64::from(v.to_bits()),
            Self::Film => bits[0] = 1,
            Self::Optics(o) => {
                bits[0] = 2;
                bits[1..17].copy_from_slice(&o.values().map(f64::to_bits));
                bits[17] = o.film_fit as u64;
                bits[18] = o.film_roll_order as u64;
            }
            Self::Unsupported => bits[0] = 3,
        }
        bits
    }
}
