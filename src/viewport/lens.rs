//! The camera lens: an explicit vertical field of view, or a film back after Maya's camera node.
//! Every film formula here is PROVISIONAL (the docs state none; see row 25 of
//! `.project-docs/development.md`) and pinned by `film_tests.rs`.
use glam::DVec2;

const MM_PER_INCH: f64 = 25.4;

/// Maya's Film Fit attribute. Fill fits the resolution gate within the film gate, Overscan the
/// film gate within the resolution gate; the viewport aspect stands in for the resolution gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmFit {
    Fill,
    Horizontal,
    Vertical,
    Overscan,
}

/// Why a film back was refused: the name of the first offending field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidFilmBack(pub &'static str);

impl std::fmt::Display for InvalidFilmBack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid film back field {}", self.0)
    }
}

impl std::error::Error for InvalidFilmBack {}

/// Maya camera film-back attributes with exact f64 storage. Units follow Maya: millimetres for
/// the focal length, inches for apertures and offsets, degrees for the roll.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilmBack {
    pub focal_mm: f64,
    /// Horizontal then vertical film aperture, inches.
    pub aperture_in: [f64; 2],
    pub film_fit: FilmFit,
    pub film_fit_offset_in: f64,
    /// Horizontal then vertical film offset, inches.
    pub film_offset_in: [f64; 2],
    pub lens_squeeze: f64,
    pub pre_scale: f64,
    pub post_scale: f64,
    pub film_roll_degrees: f64,
    pub overscan: f64,
    pub display_film_gate: bool,
}

impl FilmBack {
    /// Create Camera Options: focal length 35, "The default values are 1.4173 and 0.9449",
    /// Fill, overscan 1.
    pub const DEFAULT: Self = Self {
        focal_mm: 35.0,
        aperture_in: [1.4173, 0.9449],
        film_fit: FilmFit::Fill,
        film_fit_offset_in: 0.0,
        film_offset_in: [0.0, 0.0],
        lens_squeeze: 1.0,
        pre_scale: 1.0,
        post_scale: 1.0,
        film_roll_degrees: 0.0,
        overscan: 1.0,
        display_film_gate: false,
    };

    /// Finite everywhere; focal length, apertures, squeeze, scales and overscan above zero.
    pub fn validate(&self) -> Result<(), InvalidFilmBack> {
        let positive = [
            ("focal_mm", self.focal_mm),
            ("horizontal_aperture", self.aperture_in[0]),
            ("vertical_aperture", self.aperture_in[1]),
            ("lens_squeeze", self.lens_squeeze),
            ("pre_scale", self.pre_scale),
            ("post_scale", self.post_scale),
            ("overscan", self.overscan),
        ];
        if let Some((name, _)) = positive.iter().find(|(_, v)| !(v.is_finite() && *v > 0.0)) {
            return Err(InvalidFilmBack(name));
        }
        let finite = [
            ("film_fit_offset", self.film_fit_offset_in),
            ("horizontal_film_offset", self.film_offset_in[0]),
            ("vertical_film_offset", self.film_offset_in[1]),
            ("film_roll", self.film_roll_degrees),
        ];
        match finite.iter().find(|(_, v)| !v.is_finite()) {
            Some((name, _)) => Err(InvalidFilmBack(name)),
            None => Ok(()),
        }
    }

    fn bits(&self) -> [u64; 13] {
        [
            self.focal_mm.to_bits(),
            self.aperture_in[0].to_bits(),
            self.aperture_in[1].to_bits(),
            self.film_fit as u64,
            self.film_fit_offset_in.to_bits(),
            self.film_offset_in[0].to_bits(),
            self.film_offset_in[1].to_bits(),
            self.lens_squeeze.to_bits(),
            self.pre_scale.to_bits(),
            self.post_scale.to_bits(),
            self.film_roll_degrees.to_bits(),
            self.overscan.to_bits(),
            u64::from(self.display_film_gate),
        ]
    }

    /// Film gate size in millimetres; the squeeze widens the effective horizontal aperture.
    fn gate_mm(&self) -> DVec2 {
        DVec2::new(self.aperture_in[0] * self.lens_squeeze, self.aperture_in[1]) * MM_PER_INCH
    }

    /// The fit actually used at this viewport aspect; Fill and Overscan resolve to an axis.
    fn effective_fit(&self, aspect: f64) -> FilmFit {
        let gate = self.gate_mm();
        let wider = aspect >= gate.x / gate.y;
        match (self.film_fit, wider) {
            (FilmFit::Fill, true) | (FilmFit::Overscan, false) => FilmFit::Horizontal,
            (FilmFit::Fill, false) | (FilmFit::Overscan, true) => FilmFit::Vertical,
            (explicit, _) => explicit,
        }
    }

    /// Half extents of the symmetric view in tangent space, before overscan.
    fn tan_half(&self, aspect: f64) -> DVec2 {
        let gate = self.gate_mm();
        match self.effective_fit(aspect) {
            FilmFit::Vertical => {
                let ty = gate.y / (2.0 * self.focal_mm);
                DVec2::new(ty * aspect, ty)
            }
            _ => {
                let tx = gate.x / (2.0 * self.focal_mm);
                DVec2::new(tx, tx / aspect)
            }
        }
    }

    /// The pinhole view at a viewport aspect.
    pub(super) fn window(&self, aspect: f64) -> FilmWindow {
        let mm = MM_PER_INCH / self.focal_mm;
        let mut centre = DVec2::new(
            self.film_offset_in[0] * self.lens_squeeze,
            self.film_offset_in[1],
        ) * mm;
        // Film Fit Offset acts on the axis the explicit fit leaves free.
        match self.film_fit {
            FilmFit::Horizontal => centre.y += self.film_fit_offset_in * mm,
            FilmFit::Vertical => centre.x += self.film_fit_offset_in * self.lens_squeeze * mm,
            FilmFit::Fill | FilmFit::Overscan => {}
        }
        FilmWindow {
            half: self.tan_half(aspect) * self.overscan,
            centre,
            scale: self.pre_scale * self.post_scale,
            roll: self.film_roll_degrees.to_radians(),
            aspect,
        }
    }
}

impl Default for FilmBack {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Tangent-space view window plus the 2D film transform: pre scale, roll in pixel space, post
/// scale. The scales are one number each, so they commute with the roll.
#[derive(Clone, Copy, Debug)]
pub(super) struct FilmWindow {
    half: DVec2,
    centre: DVec2,
    scale: f64,
    roll: f64,
    aspect: f64,
}

impl FilmWindow {
    /// The 2x2 acting on (u, v), in rows: D^-1 R(roll) D scaled, D = diag(aspect, 1).
    fn linear(&self) -> [[f64; 2]; 2] {
        let (sin, cos) = self.roll.sin_cos();
        let s = self.scale;
        [
            [s * cos, -s * sin / self.aspect],
            [s * sin * self.aspect, s * cos],
        ]
    }

    /// The tangent-space window centre: the offset, plus Film Fit Offset for an explicit fit.
    pub(super) fn centre(&self) -> DVec2 {
        self.centre
    }

    /// The xy parts of the projection's x, y and z columns (clip.w = -z):
    /// clip.xy = M * ([x/tx, y/ty] + [cx/tx, cy/ty] * z).
    pub(super) fn columns(&self) -> [DVec2; 3] {
        let m = self.linear();
        let c = self.centre / self.half;
        [
            DVec2::new(m[0][0], m[1][0]) / self.half.x,
            DVec2::new(m[0][1], m[1][1]) / self.half.y,
            DVec2::new(m[0][0] * c.x + m[0][1] * c.y, m[1][0] * c.x + m[1][1] * c.y),
        ]
    }

    /// Analytic inverse of `ndc_of_tan`: the tangent-space point a pixel looks at.
    pub(super) fn tan_of_ndc(&self, ndc: DVec2) -> DVec2 {
        let m = self.linear();
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let uv = DVec2::new(
            (m[1][1] * ndc.x - m[0][1] * ndc.y) / det,
            (-m[1][0] * ndc.x + m[0][0] * ndc.y) / det,
        );
        uv * self.half + self.centre
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lens {
    /// Vertical field of view in radians, whatever the viewport aspect.
    VerticalFov(f32),
    /// A film back fitted to the viewport as the resolution gate.
    Film(FilmBack),
}

impl Lens {
    /// The lens an orthographic view uses: Maya's top, front and side cameras have their own
    /// attributes, so the film back is ignored there (PROVISIONAL).
    pub(super) fn for_orthographic(self) -> Self {
        match self {
            Self::Film(_) => Self::Film(FilmBack::DEFAULT),
            fov => fov,
        }
    }

    /// Half the vertical field of view in radians at the given viewport aspect.
    pub(super) fn half_angle_y(self, aspect: f32) -> f32 {
        match self {
            Self::VerticalFov(fov_y) => fov_y * 0.5,
            Self::Film(_) => self.tan_half_y(aspect).atan(),
        }
    }

    /// Tangent of `half_angle_y`: the symmetric vertical half extent without offset, roll,
    /// scales or overscan, lens squeeze included.
    pub(super) fn tan_half_y(self, aspect: f32) -> f32 {
        match self {
            Self::VerticalFov(fov_y) => (fov_y * 0.5).tan(),
            Self::Film(film) => film.tan_half(f64::from(aspect)).y as f32,
        }
    }

    pub(super) fn is_valid(self) -> bool {
        match self {
            Self::VerticalFov(fov_y) => {
                fov_y.is_finite() && fov_y > 0.0 && fov_y < std::f32::consts::PI
            }
            Self::Film(film) => film.validate().is_ok(),
        }
    }

    /// Discriminant and value bits, for the bit-exact `Camera::same_capture`.
    pub(super) fn capture_bits(self) -> (u8, [u64; 13]) {
        match self {
            Self::VerticalFov(fov_y) => {
                let mut bits = [0; 13];
                bits[0] = u64::from(fov_y.to_bits());
                (0, bits)
            }
            Self::Film(film) => (1, film.bits()),
        }
    }
}
