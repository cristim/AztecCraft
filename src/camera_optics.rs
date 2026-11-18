//! Full-precision camera-shape optics. No scene, viewport or file ownership lives here.
use crate::viewport::{
    MAX_FILM_SCALE, MAX_OFFSET_TAN, MAX_OVERSCAN, MAX_TAN_HALF, MIN_FILM_SCALE, MIN_TAN_HALF,
};
use glam::{DMat4, DVec4, Mat4};
#[cfg(feature = "mcp")]
use rmcp::schemars;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maya camera `filmFit` values, in the node reference's numeric order.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilmFit {
    Fill,
    Horizontal,
    Vertical,
    Overscan,
}

#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilmRollOrder {
    RotateTranslate,
    TranslateRotate,
}

/// Source units are independent of scene units: mm focal length, inches aperture/offset,
/// radians film roll, normalized post-projection translations and pivots.
#[cfg_attr(feature = "mcp", derive(schemars::JsonSchema))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraOptics {
    pub focal_length_mm: f64,
    pub aperture_in: [f64; 2],
    pub film_fit: FilmFit,
    pub film_offset_in: [f64; 2],
    pub film_fit_offset_in: f64,
    pub lens_squeeze_ratio: f64,
    pub overscan: f64,
    pub camera_scale: f64,
    pub pre_scale: f64,
    pub post_scale: f64,
    pub film_translate: [f64; 2],
    pub film_roll_radians: f64,
    pub film_roll_pivot: [f64; 2],
    pub film_roll_order: FilmRollOrder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OpticalProjection {
    Perspective,
    /// World-space width before overscan; optical focal length does not change ortho width.
    Orthographic {
        width: f64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidOptics;
impl fmt::Display for InvalidOptics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("camera optics require finite values, positive aperture, focal length, squeeze, overscan and scales, and a representable projection")
    }
}
impl std::error::Error for InvalidOptics {}

impl Default for CameraOptics {
    /// Create Camera Options defaults; the camera node's fit default differs.
    fn default() -> Self {
        Self {
            focal_length_mm: 35.0,
            aperture_in: [1.4173, 0.9449],
            film_fit: FilmFit::Fill,
            film_offset_in: [0.0; 2],
            film_fit_offset_in: 0.0,
            lens_squeeze_ratio: 1.0,
            overscan: 1.0,
            camera_scale: 1.0,
            pre_scale: 1.0,
            post_scale: 1.0,
            film_translate: [0.0; 2],
            film_roll_radians: 0.0,
            film_roll_pivot: [0.0; 2],
            film_roll_order: FilmRollOrder::RotateTranslate,
        }
    }
}

impl PartialEq for CameraOptics {
    fn eq(&self, other: &Self) -> bool {
        self.film_fit == other.film_fit
            && self.film_roll_order == other.film_roll_order
            && self.values().map(f64::to_bits) == other.values().map(f64::to_bits)
    }
}

impl CameraOptics {
    pub fn values(self) -> [f64; 16] {
        [
            self.focal_length_mm,
            self.aperture_in[0],
            self.aperture_in[1],
            self.film_offset_in[0],
            self.film_offset_in[1],
            self.film_fit_offset_in,
            self.lens_squeeze_ratio,
            self.overscan,
            self.camera_scale,
            self.pre_scale,
            self.post_scale,
            self.film_translate[0],
            self.film_translate[1],
            self.film_roll_radians,
            self.film_roll_pivot[0],
            self.film_roll_pivot[1],
        ]
    }

    pub fn node_default() -> Self {
        Self {
            film_fit: FilmFit::Horizontal,
            ..Self::default()
        }
    }

    /// Keep full values; validation never clamps or normalizes source data.
    pub fn validate(&self) -> Result<(), InvalidOptics> {
        let positive = [
            self.focal_length_mm,
            self.aperture_in[0],
            self.aperture_in[1],
            self.lens_squeeze_ratio,
            self.overscan,
            self.camera_scale,
            self.pre_scale,
            self.post_scale,
        ];
        let finite = [
            self.film_offset_in[0],
            self.film_offset_in[1],
            self.film_fit_offset_in,
            self.film_translate[0],
            self.film_translate[1],
            self.film_roll_radians,
            self.film_roll_pivot[0],
            self.film_roll_pivot[1],
        ];
        if positive.into_iter().any(|v| !v.is_finite() || v <= 0.0)
            || finite.into_iter().any(|v| !v.is_finite())
        {
            return Err(InvalidOptics);
        }
        let to_tangent = 25.4 / self.focal_length_mm * self.camera_scale;
        let horizontal = self.aperture_in[0] * self.lens_squeeze_ratio;
        let vertical = self.aperture_in[1];
        let aperture = match self.film_fit {
            FilmFit::Horizontal => horizontal,
            FilmFit::Vertical => vertical,
            FilmFit::Fill => horizontal.min(vertical),
            FilmFit::Overscan => horizontal.max(vertical),
        };
        let offsets = [
            self.film_offset_in[0],
            self.film_offset_in[1],
            self.film_fit_offset_in,
        ];
        if self.overscan > MAX_OVERSCAN
            || ![self.pre_scale, self.post_scale]
                .into_iter()
                .all(|scale| (MIN_FILM_SCALE..=MAX_FILM_SCALE).contains(&scale))
            || !(MIN_TAN_HALF..=MAX_TAN_HALF).contains(&(aperture * 0.5 * to_tangent))
            || offsets
                .into_iter()
                .any(|offset| !(0.0..=MAX_OFFSET_TAN).contains(&(offset.abs() * to_tangent)))
        {
            return Err(InvalidOptics);
        }
        Ok(())
    }

    /// Near-plane bounds (left, right, bottom, top). Squeeze expands horizontal film width:
    /// PROVISIONAL until a pinned Maya viewport comparison; see camera-film-back.md.
    pub fn frustum(&self, aspect: f64, near: f64) -> Result<[f64; 4], InvalidOptics> {
        self.validate()?;
        if !aspect.is_finite() || aspect <= 0.0 || !near.is_finite() || near <= 0.0 {
            return Err(InvalidOptics);
        }
        let width = self.aperture_in[0] * self.lens_squeeze_ratio;
        let height = self.aperture_in[1];
        let horizontal_height = width / aspect;
        let fitted_height = match self.film_fit {
            FilmFit::Horizontal => horizontal_height,
            FilmFit::Vertical => height,
            FilmFit::Fill => horizontal_height.min(height),
            FilmFit::Overscan => horizontal_height.max(height),
        };
        let fitted_width = fitted_height * aspect;
        let factor = near / self.focal_length_mm * 25.4 * self.camera_scale;
        let half_x = fitted_width * 0.5 * self.overscan * factor;
        let half_y = fitted_height * 0.5 * self.overscan * factor;
        let [mut x, mut y] = self.film_offset_in;
        match self.film_fit {
            FilmFit::Horizontal => y += self.film_fit_offset_in,
            FilmFit::Vertical => x += self.film_fit_offset_in,
            _ => {}
        }
        let [x, y] = [x * factor, y * factor];
        let bounds = [x - half_x, x + half_x, y - half_y, y + half_y];
        if bounds.iter().any(|v| !v.is_finite()) || bounds[0] >= bounds[1] || bounds[2] >= bounds[3]
        {
            return Err(InvalidOptics);
        }
        Ok(bounds)
    }

    /// Right-handed projection, NDC Z 0..1. Post projection only transforms clip X/Y;
    /// perspective depth and clipping are unchanged. Orthographic width/offset policy,
    /// matrix order/normalized pivot policy is
    /// PROVISIONAL pending reference comparison; pre/post scale precedes/follows roll.
    pub fn projection(
        &self,
        aspect: f64,
        near: f64,
        far: f64,
        mode: OpticalProjection,
    ) -> Result<Mat4, InvalidOptics> {
        let [l, r, b, t] = self.frustum(aspect, near)?;
        if !far.is_finite() || far <= near {
            return Err(InvalidOptics);
        }
        let base = match mode {
            OpticalProjection::Perspective => DMat4::from_cols(
                DVec4::new(2.0 * near / (r - l), 0.0, 0.0, 0.0),
                DVec4::new(0.0, 2.0 * near / (t - b), 0.0, 0.0),
                DVec4::new(
                    (r + l) / (r - l),
                    (t + b) / (t - b),
                    far / (near - far),
                    -1.0,
                ),
                DVec4::new(0.0, 0.0, near * far / (near - far), 0.0),
            ),
            OpticalProjection::Orthographic { width } => {
                if !width.is_finite() || width <= 0.0 {
                    return Err(InvalidOptics);
                }
                let scale = width * self.overscan * self.camera_scale;
                DMat4::from_cols(
                    DVec4::new(2.0 / scale, 0.0, 0.0, 0.0),
                    DVec4::new(0.0, 2.0 * aspect / scale, 0.0, 0.0),
                    DVec4::new(0.0, 0.0, 1.0 / (near - far), 0.0),
                    DVec4::new(
                        -(r + l) / (r - l),
                        -(t + b) / (t - b),
                        near / (near - far),
                        1.0,
                    ),
                )
            }
        };
        let translate =
            |x, y| DMat4::from_cols(DVec4::X, DVec4::Y, DVec4::Z, DVec4::new(x, y, 0.0, 1.0));
        let scale = |v| DMat4::from_diagonal(DVec4::new(v, v, 1.0, 1.0));
        let [px, py] = self.film_roll_pivot;
        let roll = translate(px, py)
            * DMat4::from_rotation_z(self.film_roll_radians.rem_euclid(std::f64::consts::TAU))
            * translate(-px, -py);
        let [x, y] = self.film_translate;
        let effect = match self.film_roll_order {
            FilmRollOrder::RotateTranslate => translate(x, y) * roll,
            FilmRollOrder::TranslateRotate => roll * translate(x, y),
        };
        let matrix = scale(self.post_scale) * effect * scale(self.pre_scale) * base;
        let output = matrix.as_mat4();
        if !matrix.is_finite() || !output.is_finite() || !output.inverse().is_finite() {
            return Err(InvalidOptics);
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec4;
    fn close(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-10, "{a} != {b}");
    }
    #[test]
    fn authored_optics_observe_the_provisional_film_back_limits() {
        let optics = CameraOptics::node_default();
        for candidate in [
            CameraOptics {
                overscan: MAX_OVERSCAN.next_up(),
                ..optics
            },
            CameraOptics {
                pre_scale: MIN_FILM_SCALE.next_down(),
                ..optics
            },
            CameraOptics {
                post_scale: MAX_FILM_SCALE.next_up(),
                ..optics
            },
            CameraOptics {
                film_offset_in: [1e6, 0.0],
                ..optics
            },
            CameraOptics {
                film_fit_offset_in: -1e6,
                ..optics
            },
            CameraOptics {
                focal_length_mm: 1e9,
                ..optics
            },
            CameraOptics {
                aperture_in: [1e9, 1.0],
                ..optics
            },
        ] {
            assert_eq!(candidate.validate(), Err(InvalidOptics));
        }
        assert!(
            CameraOptics {
                overscan: MAX_OVERSCAN,
                ..optics
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn four_fits_have_independent_gate_bounds_and_identical_equal_aspect() {
        let mut o = CameraOptics {
            aperture_in: [2.0, 1.0],
            focal_length_mm: 25.4,
            ..CameraOptics::default()
        };
        for (aspect, horizontal, vertical) in [(1.0, 2.0, 1.0), (2.0, 1.0, 1.0), (4.0, 0.5, 1.0)] {
            for (fit, height) in [
                (FilmFit::Horizontal, horizontal),
                (FilmFit::Vertical, vertical),
                (FilmFit::Fill, f64::min(horizontal, vertical)),
                (FilmFit::Overscan, f64::max(horizontal, vertical)),
            ] {
                o.film_fit = fit;
                let [l, r, b, t] = o.frustum(aspect, 1.0).unwrap();
                close(r - l, height * aspect);
                close(t - b, height);
                close(l, -r);
                close(b, -t);
            }
        }
    }
    #[test]
    fn offset_corners_and_inverse_ray_agree_without_changing_depth() {
        let o = CameraOptics {
            aperture_in: [2.0, 2.0],
            film_offset_in: [0.25, -0.125],
            focal_length_mm: 25.4,
            film_fit: FilmFit::Horizontal,
            ..CameraOptics::default()
        };
        let m = o
            .projection(1.0, 1.0, 101.0, OpticalProjection::Perspective)
            .unwrap();
        for (point, expected) in [
            (
                Vec4::new(-0.75, -1.125, -1.0, 1.0),
                Vec4::new(-1.0, -1.0, 0.0, 1.0),
            ),
            (
                Vec4::new(1.25, 0.875, -1.0, 1.0),
                Vec4::new(1.0, 1.0, 0.0, 1.0),
            ),
        ] {
            assert!((m * point - expected).abs().max_element() < 1e-6);
        }
        let p = m.inverse() * Vec4::new(0.0, 0.0, 0.0, 1.0);
        assert!(
            (p / p.w - Vec4::new(0.25, -0.125, -1.0, 1.0))
                .abs()
                .max_element()
                < 1e-6
        );
        let mut effect = o;
        effect.film_translate = [0.2, 0.3];
        effect.film_roll_radians = 1.0;
        let e = effect
            .projection(1.0, 1.0, 101.0, OpticalProjection::Perspective)
            .unwrap();
        assert_eq!(m.z_axis.z, e.z_axis.z);
        assert_eq!(m.w_axis.z, e.w_axis.z);
    }
    #[test]
    fn roll_order_pivot_and_scales_are_noncommutative() {
        let mut o = CameraOptics {
            film_translate: [0.5, 0.0],
            film_roll_radians: std::f64::consts::FRAC_PI_2,
            pre_scale: 2.0,
            post_scale: 3.0,
            ..CameraOptics::default()
        };
        let a = o
            .projection(1.0, 1.0, 100.0, OpticalProjection::Perspective)
            .unwrap()
            * Vec4::new(0.0, 0.0, -1.0, 1.0);
        assert!((a.x - 1.5).abs() < 1e-5);
        assert!(a.y.abs() < 1e-5);
        o.film_roll_order = FilmRollOrder::TranslateRotate;
        let b = o
            .projection(1.0, 1.0, 100.0, OpticalProjection::Perspective)
            .unwrap()
            * Vec4::new(0.0, 0.0, -1.0, 1.0);
        assert!(b.x.abs() < 1e-5);
        assert!((b.y - 1.5).abs() < 1e-5);
        o.film_roll_pivot = [0.5, 0.0];
        assert_ne!(
            o.projection(1.0, 1.0, 100.0, OpticalProjection::Perspective)
                .unwrap()
                * Vec4::new(0.0, 0.0, -1.0, 1.0),
            b
        );
    }
    #[test]
    fn optical_factors_offsets_and_orthographic_bounds() {
        let base = CameraOptics {
            aperture_in: [2.0, 2.0],
            focal_length_mm: 25.4,
            film_fit: FilmFit::Horizontal,
            ..CameraOptics::default()
        };
        for (squeeze, overscan, scale, expected) in [
            (2.0, 1.0, 1.0, 4.0),
            (1.0, 2.0, 1.0, 4.0),
            (1.0, 1.0, 2.0, 4.0),
        ] {
            let o = CameraOptics {
                lens_squeeze_ratio: squeeze,
                overscan,
                camera_scale: scale,
                ..base
            };
            let [l, r, b, t] = o.frustum(1.0, 1.0).unwrap();
            close(r - l, expected);
            close(t - b, expected);
        }
        let mut o = CameraOptics {
            film_fit_offset_in: 0.25,
            ..base
        };
        let [l, r, b, t] = o.frustum(1.0, 1.0).unwrap();
        close(l + r, 0.0);
        close(b + t, 0.5);
        o.film_fit = FilmFit::Vertical;
        let [l, r, b, t] = o.frustum(1.0, 1.0).unwrap();
        close(l + r, 0.5);
        close(b + t, 0.0);
        o.film_fit = FilmFit::Fill;
        assert_eq!(
            o.frustum(1.0, 1.0).unwrap(),
            base.frustum(1.0, 1.0).unwrap()
        );
        let m = base
            .projection(
                2.0,
                1.0,
                101.0,
                OpticalProjection::Orthographic { width: 8.0 },
            )
            .unwrap();
        assert!(
            (m * Vec4::new(4.0, 2.0, -1.0, 1.0) - Vec4::new(1.0, 1.0, 0.0, 1.0))
                .abs()
                .max_element()
                < 1e-6
        );
        assert!(
            (m * Vec4::new(-4.0, -2.0, -101.0, 1.0) - Vec4::new(-1.0, -1.0, 1.0, 1.0))
                .abs()
                .max_element()
                < 1e-6
        );
        let p = m.inverse() * Vec4::new(1.0, 1.0, 0.0, 1.0);
        assert!((p - Vec4::new(4.0, 2.0, -1.0, 1.0)).abs().max_element() < 1e-5);
        for field in [
            "focal_length_mm",
            "lens_squeeze_ratio",
            "overscan",
            "camera_scale",
            "pre_scale",
            "post_scale",
        ] {
            for value in [0.0, -1.0, f64::MAX, f64::MIN_POSITIVE] {
                let mut json = serde_json::to_value(base).unwrap();
                json[field] = serde_json::json!(value);
                let bad: CameraOptics = serde_json::from_value(json).unwrap();
                assert!(
                    bad.projection(1.0, 0.1, 10000.0, OpticalProjection::Perspective)
                        .is_err(),
                    "{field} {value}"
                );
            }
        }
        let mut invalid = base;
        for index in 0..2 {
            invalid.aperture_in[index] = f64::NAN;
            assert!(invalid.validate().is_err());
            invalid = base;
            invalid.film_offset_in[index] = f64::NAN;
            assert!(invalid.validate().is_err());
            invalid = base;
            invalid.film_translate[index] = f64::INFINITY;
            assert!(invalid.validate().is_err());
            invalid = base;
            invalid.film_roll_pivot[index] = f64::INFINITY;
            assert!(invalid.validate().is_err());
            invalid = base;
        }
        invalid.film_fit_offset_in = f64::NAN;
        assert!(invalid.validate().is_err());
        invalid = base;
        invalid.film_roll_radians = f64::INFINITY;
        assert!(invalid.validate().is_err());
        let negzero = CameraOptics {
            film_offset_in: [-0.0, 0.0],
            ..base
        };
        assert_ne!(base, negzero);
    }

    #[test]
    fn boundaries_and_source_precision() {
        let o = CameraOptics::default();
        for aspect in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            f64::MIN_POSITIVE,
        ] {
            assert!(
                o.projection(aspect, 0.1, 10000.0, OpticalProjection::Perspective)
                    .is_err()
            );
        }
        for (near, far) in [
            (0.0, 1.0),
            (-1.0, 1.0),
            (1.0, 1.0),
            (2.0, 1.0),
            (f64::NAN, 100.0),
            (1.0, f64::INFINITY),
        ] {
            assert!(
                o.projection(1.0, near, far, OpticalProjection::Perspective)
                    .is_err()
            );
        }
        for v in [
            0.0,
            -1.0,
            f64::NAN,
            f64::INFINITY,
            f64::MAX,
            f64::MIN_POSITIVE,
        ] {
            let mut bad = o;
            bad.focal_length_mm = v;
            assert!(
                bad.projection(1.0, 0.1, 10000.0, OpticalProjection::Perspective)
                    .is_err()
            );
        }
        assert!(
            o.projection(
                1.0,
                0.1,
                100.0,
                OpticalProjection::Orthographic { width: 0.0 }
            )
            .is_err()
        );
        let source = CameraOptics {
            focal_length_mm: 35.123456789012345,
            film_offset_in: [-0.0, 0.0],
            ..o
        };
        let encoded = serde_json::to_string(&source).unwrap();
        let decoded: CameraOptics = serde_json::from_str(&encoded).unwrap();
        assert_eq!(
            source.focal_length_mm.to_bits(),
            decoded.focal_length_mm.to_bits()
        );
        assert_eq!(decoded.film_offset_in[0].to_bits(), (-0.0f64).to_bits());
        assert!(serde_json::from_str::<FilmFit>("\"invalid\"").is_err());
        assert_ne!(
            CameraOptics::default().film_fit,
            CameraOptics::node_default().film_fit
        );
    }
}
