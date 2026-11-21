//! The camera lens: an explicit vertical field of view, or a film back after Maya's camera node.
//! Every film formula here is PROVISIONAL (the docs state none; see row 25 of
//! `.project-docs/development.md`) and pinned by `film_tests.rs`.
use glam::{DVec2, Vec2};

const MM_PER_INCH: f64 = 25.4;

/// Largest Overscan and 2D scale (PROVISIONAL: the docs state no range; development.md row 56).
pub const MAX_OVERSCAN: f64 = 1e3;
pub const MAX_FILM_SCALE: f64 = 1e6;
pub const MIN_FILM_SCALE: f64 = 1e-6;
/// Largest film or fit offset in tangent space (offset * 25.4 / focal length), ten times the
/// largest half extent (PROVISIONAL, row 56): the z column of the projection stays finite at
/// aspects from 1e-3 to 1e3.
pub const MAX_OFFSET_TAN: f64 = 1e5;
/// Largest Film Translate or Film Roll Pivot component, in NDC (PROVISIONAL, row 56): ten times
/// the largest half extent of the offset bound, so the projection's z column stays finite.
pub const MAX_NDC_SHIFT: f64 = 1e5;
/// Bounds of the symmetric half extent in tangent space at aspect 1, overscan excluded
/// (PROVISIONAL, row 56): about 0.006 degrees to 89.994 degrees of half angle.
pub const MIN_TAN_HALF: f64 = 1e-4;
pub const MAX_TAN_HALF: f64 = 1e4;

/// Maya's Film Fit attribute. Fill fits the resolution gate within the film gate, Overscan the
/// film gate within the resolution gate; the viewport aspect stands in for the resolution gate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmFit {
    Fill,
    Horizontal,
    Vertical,
    Overscan,
}

/// Maya's Film Roll Order: whether the roll applies before or after the Film Translate. The ids
/// (0 rotate-translate, 1 translate-rotate) and which composition each name means are PROVISIONAL:
/// the node page lists no ids and the command page is inconsistent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmRollOrder {
    /// Roll about the pivot, then translate.
    RotateTranslate,
    /// Translate, then roll about the pivot.
    TranslateRotate,
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
    /// Scales the focal length, effective focal length = focal * scale (PROVISIONAL direction).
    pub camera_scale: f64,
    /// Horizontal then vertical Film Translate, normalized device units (PROVISIONAL unit).
    pub film_translate: [f64; 2],
    /// Horizontal then vertical Film Roll Pivot, normalized device units (PROVISIONAL unit).
    pub film_roll_pivot: [f64; 2],
    pub film_roll_order: FilmRollOrder,
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
        camera_scale: 1.0,
        film_translate: [0.0, 0.0],
        film_roll_pivot: [0.0, 0.0],
        film_roll_order: FilmRollOrder::RotateTranslate,
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
            ("camera_scale", self.camera_scale),
        ];
        if let Some((name, _)) = positive.iter().find(|(_, v)| !(v.is_finite() && *v > 0.0)) {
            return Err(InvalidFilmBack(name));
        }
        let ranged = [
            ("overscan", self.overscan, 0.0, MAX_OVERSCAN),
            ("pre_scale", self.pre_scale, MIN_FILM_SCALE, MAX_FILM_SCALE),
            (
                "camera_scale",
                self.camera_scale,
                MIN_FILM_SCALE,
                MAX_FILM_SCALE,
            ),
            (
                "post_scale",
                self.post_scale,
                MIN_FILM_SCALE,
                MAX_FILM_SCALE,
            ),
        ];
        if let Some((name, ..)) = ranged
            .iter()
            .find(|(_, v, min, max)| !(*min..=*max).contains(v))
        {
            return Err(InvalidFilmBack(name));
        }
        let finite = [
            ("film_fit_offset", self.film_fit_offset_in),
            ("horizontal_film_offset", self.film_offset_in[0]),
            ("vertical_film_offset", self.film_offset_in[1]),
            ("film_roll", self.film_roll_degrees),
        ];
        if let Some((name, _)) = finite.iter().find(|(_, v)| !v.is_finite()) {
            return Err(InvalidFilmBack(name));
        }
        let shifts = [
            ("film_translate_h", self.film_translate[0]),
            ("film_translate_v", self.film_translate[1]),
            ("horizontal_roll_pivot", self.film_roll_pivot[0]),
            ("vertical_roll_pivot", self.film_roll_pivot[1]),
        ];
        if let Some((name, _)) = shifts
            .iter()
            .find(|(_, v)| !(v.is_finite() && v.abs() <= MAX_NDC_SHIFT))
        {
            return Err(InvalidFilmBack(name));
        }
        let half = self.tan_half(1.0);
        if !(MIN_TAN_HALF..=MAX_TAN_HALF).contains(&half.x) {
            return Err(InvalidFilmBack("angle_of_view"));
        }
        let to_tan = MM_PER_INCH / self.focal_eff();
        let offsets = [
            (
                "film_fit_offset",
                self.film_fit_offset_in * self.lens_squeeze.max(1.0),
            ),
            (
                "horizontal_film_offset",
                self.film_offset_in[0] * self.lens_squeeze,
            ),
            ("vertical_film_offset", self.film_offset_in[1]),
        ];
        if let Some((name, _)) = offsets
            .iter()
            .find(|(_, v)| !(0.0..=MAX_OFFSET_TAN).contains(&(v.abs() * to_tan)))
        {
            return Err(InvalidFilmBack(name));
        }
        // The f32 projection must stay usable: finite entries and a nonzero xy determinant
        // (a focal length of 1e-300 or pre * post of 1e-200 underflows or overflows f32).
        let [x, y, z] = self.window(1.0).columns().map(|c| c.as_vec2());
        let det = x.x * y.y - y.x * x.y;
        if ![x, y, z].iter().all(|c| c.is_finite()) || !det.is_finite() || det == 0.0 {
            return Err(InvalidFilmBack("projection"));
        }
        Ok(())
    }

    fn bits(&self) -> [u64; 19] {
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
            self.camera_scale.to_bits(),
            self.film_translate[0].to_bits(),
            self.film_translate[1].to_bits(),
            self.film_roll_pivot[0].to_bits(),
            self.film_roll_pivot[1].to_bits(),
            self.film_roll_order as u64,
        ]
    }

    /// Focal length times the camera scale (PROVISIONAL direction).
    fn focal_eff(&self) -> f64 {
        self.focal_mm * self.camera_scale
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
                let ty = gate.y / (2.0 * self.focal_eff());
                DVec2::new(ty * aspect, ty)
            }
            _ => {
                let tx = gate.x / (2.0 * self.focal_eff());
                DVec2::new(tx, tx / aspect)
            }
        }
    }

    /// Film Offset in tangent space, squeeze on the horizontal part (PROVISIONAL).
    fn offset_tan(&self) -> DVec2 {
        DVec2::new(
            self.film_offset_in[0] * self.lens_squeeze,
            self.film_offset_in[1],
        ) * (MM_PER_INCH / self.focal_eff())
    }

    /// The pinhole view at a viewport aspect.
    pub(super) fn window(&self, aspect: f64) -> FilmWindow {
        let mm = MM_PER_INCH / self.focal_eff();
        let mut centre = self.offset_tan();
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
            translate: DVec2::from_array(self.film_translate),
            pivot: DVec2::from_array(self.film_roll_pivot),
            order: self.film_roll_order,
            post_scale: self.post_scale,
        }
    }
}

/// A partial film back: only the `Some` fields change. All-or-nothing once validated.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FilmBackEdit {
    pub focal_mm: Option<f64>,
    pub aperture_in: [Option<f64>; 2],
    pub film_fit: Option<FilmFit>,
    pub film_fit_offset_in: Option<f64>,
    pub film_offset_in: [Option<f64>; 2],
    pub lens_squeeze: Option<f64>,
    pub pre_scale: Option<f64>,
    pub post_scale: Option<f64>,
    pub film_roll_degrees: Option<f64>,
    pub overscan: Option<f64>,
    pub display_film_gate: Option<bool>,
    pub camera_scale: Option<f64>,
    pub film_translate: [Option<f64>; 2],
    pub film_roll_pivot: [Option<f64>; 2],
    pub film_roll_order: Option<FilmRollOrder>,
}

impl FilmBackEdit {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The film back after the edit, not yet validated. A plain attribute edit: the Film Gate menu
    /// item (not `camera -e -displayFilmGate`) is what sets Overscan 1.5, so it sends both fields.
    pub fn applied_to(&self, base: FilmBack) -> FilmBack {
        let mut film = base;
        let set = |slot: &mut f64, value: Option<f64>| {
            if let Some(value) = value {
                *slot = value;
            }
        };
        set(&mut film.focal_mm, self.focal_mm);
        set(&mut film.aperture_in[0], self.aperture_in[0]);
        set(&mut film.aperture_in[1], self.aperture_in[1]);
        set(&mut film.film_fit_offset_in, self.film_fit_offset_in);
        set(&mut film.film_offset_in[0], self.film_offset_in[0]);
        set(&mut film.film_offset_in[1], self.film_offset_in[1]);
        set(&mut film.lens_squeeze, self.lens_squeeze);
        set(&mut film.pre_scale, self.pre_scale);
        set(&mut film.post_scale, self.post_scale);
        set(&mut film.film_roll_degrees, self.film_roll_degrees);
        set(&mut film.overscan, self.overscan);
        set(&mut film.camera_scale, self.camera_scale);
        set(&mut film.film_translate[0], self.film_translate[0]);
        set(&mut film.film_translate[1], self.film_translate[1]);
        set(&mut film.film_roll_pivot[0], self.film_roll_pivot[0]);
        set(&mut film.film_roll_pivot[1], self.film_roll_pivot[1]);
        if let Some(order) = self.film_roll_order {
            film.film_roll_order = order;
        }
        if let Some(fit) = self.film_fit {
            film.film_fit = fit;
        }
        if let Some(on) = self.display_film_gate {
            film.display_film_gate = on;
        }
        film
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
    translate: DVec2,
    pivot: DVec2,
    order: FilmRollOrder,
    post_scale: f64,
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

    /// The NDC shift after the linear part: post scale times the pivot-relative roll residue plus
    /// the Film Translate, in the order Film Roll Order names (PROVISIONAL). Zero at the neutral
    /// values, so the pre-existing projection is unchanged bit for bit.
    fn shift(&self) -> DVec2 {
        if self.translate == DVec2::ZERO && self.pivot == DVec2::ZERO {
            return DVec2::ZERO;
        }
        let (sin, cos) = self.roll.sin_cos();
        let rotate = |v: DVec2| {
            DVec2::new(
                cos * v.x - sin * v.y / self.aspect,
                sin * v.x * self.aspect + cos * v.y,
            )
        };
        let moved = match self.order {
            FilmRollOrder::RotateTranslate => self.translate,
            FilmRollOrder::TranslateRotate => rotate(self.translate),
        };
        (self.pivot - rotate(self.pivot) + moved) * self.post_scale
    }

    /// Vertical half extent in tangent space as drawn: overscan and the 2D scales included, a
    /// roll ignored.
    pub(super) fn visible_tan_half_y(&self) -> f64 {
        self.half.y / self.scale
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
            DVec2::new(m[0][0] * c.x + m[0][1] * c.y, m[1][0] * c.x + m[1][1] * c.y) - self.shift(),
        ]
    }

    /// NDC of a point in tangent space (x/-z, y/-z).
    fn ndc_of_tan(&self, tan: DVec2) -> DVec2 {
        let m = self.linear();
        let uv = (tan - self.centre) / self.half;
        DVec2::new(
            m[0][0] * uv.x + m[0][1] * uv.y,
            m[1][0] * uv.x + m[1][1] * uv.y,
        ) + self.shift()
    }

    /// Analytic inverse of `ndc_of_tan`: the tangent-space point a pixel looks at.
    pub(super) fn tan_of_ndc(&self, ndc: DVec2) -> DVec2 {
        let m = self.linear();
        let det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
        let ndc = ndc - self.shift();
        let uv = DVec2::new(
            (m[1][1] * ndc.x - m[0][1] * ndc.y) / det,
            (-m[1][0] * ndc.x + m[0][0] * ndc.y) / det,
        );
        uv * self.half + self.centre
    }
}

/// The film gate corners in NDC at the given viewport aspect, lower-left, lower-right,
/// upper-right, upper-left in film space (before the roll). The gate is centred on the squeezed
/// Film Offset, so it moves with the scene and only Film Fit Offset shifts it in the view
/// (the Film Back page: Film Offset offsets the gates relative to the scene). Overscan, scales,
/// roll (PROVISIONAL for the gate) and the fit rules apply; the width uses the squeezed
/// aperture. Computed in f64, cast once.
pub fn film_gate_ndc(film: &FilmBack, viewport_aspect: f32) -> [Vec2; 4] {
    let window = film.window(f64::from(viewport_aspect));
    let half = film.gate_mm() / (2.0 * film.focal_eff());
    let centre = film.offset_tan();
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)].map(|(sx, sy)| {
        let ndc = window.ndc_of_tan(centre + DVec2::new(sx * half.x, sy * half.y));
        Vec2::new(ndc.x as f32, ndc.y as f32)
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lens {
    /// Vertical field of view in radians, whatever the viewport aspect.
    VerticalFov(f32),
    /// A film back fitted to the viewport as the resolution gate.
    Film(FilmBack),
}

impl Lens {
    /// A field of view alone does not determine focal length without a film aperture.
    pub fn focal_length_mm(self) -> Option<f32> {
        match self {
            Self::Film(film) => Some(film.focal_mm as f32),
            Self::VerticalFov(_) => None,
        }
    }

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
    pub(super) fn capture_bits(self) -> (u8, [u64; 19]) {
        match self {
            Self::VerticalFov(fov_y) => {
                let mut bits = [0; 19];
                bits[0] = u64::from(fov_y.to_bits());
                (0, bits)
            }
            Self::Film(film) => (1, film.bits()),
        }
    }
}
