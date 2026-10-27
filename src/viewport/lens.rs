//! The camera lens: an explicit vertical field of view, or the film back of Maya's default camera.

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
            Self::Film => self.tan_half_y(aspect).atan(),
        }
    }

    /// Tangent of `half_angle_y`; the projection scale is its reciprocal.
    pub(super) fn tan_half_y(self, aspect: f32) -> f32 {
        match self {
            Self::VerticalFov(fov_y) => (fov_y * 0.5).tan(),
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
        }
    }

    /// Discriminant and angle bits, for the bit-exact `Camera::same_capture`.
    pub(super) fn capture_bits(self) -> (u8, u32) {
        match self {
            Self::VerticalFov(fov_y) => (0, fov_y.to_bits()),
            Self::Film => (1, 0),
        }
    }
}
