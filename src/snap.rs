//! Snap to grids for the Move tool: the lead object's pivot lands on a grid line of the drawn
//! grid. Pure math over world centimetres; the editor feeds it the lead pivot and handle axis.

use crate::native_scene::{NativeError, Units};
use glam::DVec3;
use std::fmt;

/// The grid options of Maya's Display > Grid option box: a main line every `lines_every` scene
/// linear units, each square split into `subdivisions`. The viewport's drawn grid and Snap to grids
/// both read the same value, so the drawn lines and the snap lines have one source.
/// The current Maya grid command page (Commands/grid.html) gives defaults 10 and 10 "in linear
/// units"; no page gives 5 and 5, which rests on a runtime `grid -q` result in Maya 2025.1.
/// The 5 and 5 defaults are PROVISIONAL (#5); the options are not editable yet (#160).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridOptions {
    pub lines_every: f64,
    pub subdivisions: u32,
}

pub const DEFAULT_GRID_OPTIONS: GridOptions = GridOptions {
    lines_every: 5.0,
    subdivisions: 5,
};

impl GridOptions {
    /// The distance between drawn lines, main and subdivision, in scene linear units.
    pub fn line_spacing_units(&self) -> f64 {
        self.lines_every / f64::from(self.subdivisions.max(1))
    }
}

/// The distance between the drawn grid's lines in centimetres: the line spacing is a length in the
/// scene linear unit (Maya's `grid` command gives its spacing "in linear units"), so a metre scene
/// draws and snaps every 100 cm x the line spacing and an inch scene every 2.54 cm x it. Snap to
/// grids rounds to this spacing and the viewport draws its lines at it. The unit factors are exact
/// (1, 0.1, 100, 2.54 and 30.48 cm per unit); an error for an unsupported linear unit, where
/// there is no grid and no snap. PROVISIONAL (#5): snapping to the subdivision (drawn) lines.
pub fn drawn_grid_spacing_cm(grid: GridOptions, units: &Units) -> Result<f64, NativeError> {
    Ok(grid.line_spacing_units() * units.linear_factor()?)
}

/// A handle axis within this of a world axis counts as aligned; an Object or Parent axis
/// rotated by a multiple of 90 degrees differs from it by rounding error only.
const AXIS_TOLERANCE: f64 = 1e-9;

/// A pivot within this fraction of max(|coordinate|, 1) of a grid line counts as on it: 32 times
/// 2^20 (the inverse of the core's 2^-20 move resolution, `MIN_RESOLUTION`) times f64 epsilon,
/// about 7.45e-9, so the stored channel's few-ulp rounding error stays well inside it.
const ON_LINE_RELATIVE: f64 = 32.0 * f64::EPSILON * 1_048_576.0;

/// The tolerance never exceeds this fraction of the line spacing, so a fine grid far from the
/// origin keeps its lines apart (at 1e6 the coordinate term alone is 7e-3, wider than a 0.001
/// spacing). PROVISIONAL (#5): no Maya source for this constant or the on-line tolerance. Where
/// the cap is below the core's move resolution at that coordinate, the small gap is requested
/// and the core refuses it loudly.
const ON_LINE_SPACING_FRACTION: f64 = 1e-2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapError {
    /// The Move axis is not along a world X, Y or Z grid axis, so no grid line lies on it.
    AxisOffGrid,
    /// An input or the snapped result is not a finite number.
    NonFinite,
    /// A step increment that is not a finite number above zero.
    InvalidStep,
    /// A Scale step that is not a finite number of at least `MIN_SCALE_STEP`.
    InvalidScaleStep,
}

impl fmt::Display for SnapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AxisOffGrid => f.write_str(
                "Snap to grids needs the Move axis along a world X, Y or Z grid axis; \
                 use World Axis Orientation or turn Snap to grids off",
            ),
            Self::NonFinite => f.write_str("Snap to grids needs finite values"),
            Self::InvalidStep => write!(
                f,
                "Rotate step must be a finite number of at least {MIN_ROTATE_STEP_DEGREES} degrees"
            ),
            Self::InvalidScaleStep => write!(
                f,
                "Scale step must be a finite number of at least {MIN_SCALE_STEP}"
            ),
        }
    }
}

/// The distance along `direction` (a unit world vector) from `origin` that puts the pivot on the
/// nearest grid line, given the unsnapped `distance`. Absolute: the result depends on where the
/// pivot is, not on the move's size alone.
pub fn snap_distance_to_grid(
    origin: DVec3,
    direction: DVec3,
    distance: f64,
    spacing: f64,
) -> Result<f64, SnapError> {
    if !(origin.is_finite() && direction.is_finite() && distance.is_finite())
        || !spacing.is_finite()
        || spacing <= 0.0
    {
        return Err(SnapError::NonFinite);
    }
    let components = direction.to_array();
    let origin = origin.to_array();
    let mut aligned = None;
    for (index, value) in components.into_iter().enumerate() {
        if (value.abs() - 1.0).abs() <= AXIS_TOLERANCE {
            aligned = Some((index, value.signum()));
        } else if value.abs() > AXIS_TOLERANCE {
            return Err(SnapError::AxisOffGrid);
        }
    }
    let (index, sign) = aligned.ok_or(SnapError::AxisOffGrid)?;
    let target = origin[index] + sign * distance;
    let snapped = (target / spacing).round() * spacing;
    let gap = snapped - origin[index];
    // `translation_in` refuses a displacement below about 2^-20 of the stored channel's rounding
    // (`MIN_RESOLUTION`). A pivot that close to a line is on it, and a refusal would fail every
    // drag update, so the distance is exactly zero.
    let tolerance =
        (ON_LINE_RELATIVE * origin[index].abs().max(1.0)).min(ON_LINE_SPACING_FRACTION * spacing);
    let result = if gap.abs() <= tolerance {
        0.0
    } else {
        gap * sign
    };
    if target.is_finite() && result.is_finite() {
        Ok(result)
    } else {
        Err(SnapError::NonFinite)
    }
}

/// The smallest Rotate step in degrees. A smaller step makes `degrees / step` overflow or lose
/// all precision, so it is refused instead of silently refusing every later rotate.
pub const MIN_ROTATE_STEP_DEGREES: f64 = 1e-6;

/// `degrees` rounded to the nearest multiple of `step` (relative step snap: the drag's turn, not
/// the object's resulting angle); a half-way turn rounds away from zero and a result of zero is
/// always `+0.0`. A turn of 2^52 steps or more is already a multiple within f64 precision and is
/// returned as is. The only validation of a step: a setter calls it with 0.
pub fn snap_angle_to_step(degrees: f64, step: f64) -> Result<f64, SnapError> {
    if !step.is_finite() || step < MIN_ROTATE_STEP_DEGREES {
        return Err(SnapError::InvalidStep);
    }
    if !degrees.is_finite() {
        return Err(SnapError::NonFinite);
    }
    let steps = degrees / step;
    if !steps.is_finite() || steps.abs() >= 4_503_599_627_370_496.0 {
        return Ok(degrees);
    }
    let snapped = steps.round() * step + 0.0;
    if snapped.is_finite() {
        Ok(snapped)
    } else {
        Err(SnapError::NonFinite)
    }
}

/// The smallest Scale step, for the same overflow reason as `MIN_ROTATE_STEP_DEGREES`.
/// PROVISIONAL: Maya's Scale tool snap value range is not confirmed against the pinned reference.
pub const MIN_SCALE_STEP: f64 = 1e-6;

/// `factor` rounded to the nearest multiple of `step` (relative step snap of the drag's scale
/// factor); a half-way factor rounds away from zero. A factor that would round to zero keeps one
/// step with its own sign, because a zero scale factor collapses the object. PROVISIONAL: whether
/// Maya snaps the factor or the resulting scale is not confirmed against the pinned reference.
pub fn snap_factor_to_step(factor: f64, step: f64) -> Result<f64, SnapError> {
    if !step.is_finite() || step < MIN_SCALE_STEP {
        return Err(SnapError::InvalidScaleStep);
    }
    if !factor.is_finite() {
        return Err(SnapError::NonFinite);
    }
    let steps = factor / step;
    if !steps.is_finite() || steps.abs() >= 4_503_599_627_370_496.0 {
        return Ok(factor);
    }
    let rounded = steps.round();
    let snapped = if rounded == 0.0 {
        step.copysign(factor)
    } else {
        rounded * step
    };
    if snapped.is_finite() {
        Ok(snapped)
    } else {
        Err(SnapError::NonFinite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_factor_snaps_to_the_nearest_step_and_never_to_zero() {
        assert_eq!(snap_factor_to_step(1.12, 0.25), Ok(1.0));
        assert_eq!(snap_factor_to_step(1.13, 0.25), Ok(1.25));
        assert_eq!(snap_factor_to_step(0.125, 0.25), Ok(0.25));
        assert_eq!(snap_factor_to_step(0.01, 0.25), Ok(0.25));
        assert_eq!(snap_factor_to_step(-0.01, 0.25), Ok(-0.25));
        assert_eq!(snap_factor_to_step(1e300, 0.25), Ok(1e300));
    }

    #[test]
    fn a_bad_scale_step_or_factor_is_refused() {
        for step in [0.0, -1.0, f64::NAN, f64::INFINITY, 9.9e-7] {
            assert_eq!(
                snap_factor_to_step(2.0, step),
                Err(SnapError::InvalidScaleStep)
            );
        }
        assert_eq!(
            snap_factor_to_step(f64::NAN, 0.5),
            Err(SnapError::NonFinite)
        );
    }

    #[test]
    fn an_angle_snaps_to_the_nearest_step_multiple() {
        assert_eq!(snap_angle_to_step(37.0, 15.0), Ok(30.0));
        assert_eq!(snap_angle_to_step(38.0, 15.0), Ok(45.0));
        assert_eq!(snap_angle_to_step(-38.0, 15.0), Ok(-45.0));
        assert_eq!(snap_angle_to_step(7.0, 15.0), Ok(0.0));
    }

    #[test]
    fn a_half_way_turn_rounds_away_from_zero_and_zero_is_positive() {
        assert_eq!(snap_angle_to_step(7.5, 15.0), Ok(15.0));
        assert_eq!(snap_angle_to_step(-7.5, 15.0), Ok(-15.0));
        let zero = snap_angle_to_step(-7.0, 15.0).unwrap();
        assert!(zero == 0.0 && zero.is_sign_positive());
    }

    #[test]
    fn a_step_above_a_full_turn_still_snaps() {
        assert_eq!(snap_angle_to_step(37.0, 400.0), Ok(0.0));
        assert_eq!(snap_angle_to_step(250.0, 400.0), Ok(400.0));
        assert_eq!(snap_angle_to_step(-250.0, 400.0), Ok(-400.0));
    }

    #[test]
    fn a_bad_step_or_angle_is_refused() {
        for step in [0.0, -5.0, f64::NAN, f64::INFINITY, 5e-324, 1e-307, 9.9e-7] {
            assert_eq!(snap_angle_to_step(10.0, step), Err(SnapError::InvalidStep));
        }
        assert_eq!(snap_angle_to_step(10.0, MIN_ROTATE_STEP_DEGREES), Ok(10.0));
        assert_eq!(snap_angle_to_step(f64::NAN, 5.0), Err(SnapError::NonFinite));
    }

    #[test]
    fn a_huge_turn_stays_finite() {
        assert_eq!(snap_angle_to_step(1.7e308, 0.5), Ok(1.7e308));
        assert_eq!(snap_angle_to_step(-1.7e308, 0.5), Ok(-1.7e308));
        assert_eq!(snap_angle_to_step(f64::MAX, 1e-6), Ok(f64::MAX));
        assert_eq!(snap_angle_to_step(1e300, 360.0), Ok(1e300));
    }

    #[test]
    fn world_axis_lands_the_pivot_on_the_nearest_line() {
        let origin = DVec3::new(0.3, 0.0, 0.0);
        let d = snap_distance_to_grid(origin, DVec3::X, 2.4, 1.0).unwrap();
        assert!((origin.x + d - 3.0).abs() < 1e-12);
        let d = snap_distance_to_grid(origin, DVec3::X, 2.1, 1.0).unwrap();
        assert!((origin.x + d - 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_negative_axis_snaps_the_world_coordinate() {
        let origin = DVec3::new(0.0, 5.25, 0.0);
        let d = snap_distance_to_grid(origin, -DVec3::Y, 1.8, 1.0).unwrap();
        assert!((origin.y - d - 3.0).abs() < 1e-12, "{d}");
    }

    #[test]
    fn a_pivot_already_on_a_line_stays_for_a_small_move() {
        assert_eq!(
            snap_distance_to_grid(DVec3::new(4.0, 0.0, 0.0), DVec3::X, 0.4, 1.0),
            Ok(0.0)
        );
    }

    #[test]
    fn a_pivot_a_few_ulps_off_a_line_gets_exactly_zero() {
        for origin in [2.000_000_000_000_000_4, 1.999_999_999_999_999_8, 1e9 + 1e-3] {
            let o = DVec3::new(origin, 0.0, 0.0);
            assert_eq!(snap_distance_to_grid(o, DVec3::X, 0.1, 1.0), Ok(0.0));
            assert_eq!(snap_distance_to_grid(o, -DVec3::X, 0.1, 1.0), Ok(0.0));
        }
        // Just outside the tolerance the real distance is returned.
        let d =
            snap_distance_to_grid(DVec3::new(2.0 + 1e-6, 0.0, 0.0), DVec3::X, 0.1, 1.0).unwrap();
        assert!((d + 1e-6).abs() < 1e-12, "{d}");
    }

    #[test]
    fn the_tolerance_follows_the_spacing_at_fine_and_coarse_grids_far_from_the_origin() {
        for spacing in [0.001_f64, 0.01, 1.0, 100.0] {
            for base in [0.0, 1.0e6] {
                let on_line = (base / spacing).round() * spacing;
                // A few ulps off a line is on it.
                let near = on_line + 3.0 * f64::EPSILON * on_line.abs().max(1.0);
                assert_eq!(
                    snap_distance_to_grid(DVec3::new(near, 0.0, 0.0), DVec3::X, 0.0, spacing),
                    Ok(0.0),
                    "spacing {spacing} base {base}"
                );
                // A tenth of a spacing off a line is not: the pivot moves onto the line.
                let off = on_line + 0.1 * spacing;
                let d = snap_distance_to_grid(DVec3::new(off, 0.0, 0.0), DVec3::X, 0.0, spacing)
                    .unwrap();
                assert!(
                    (d + 0.1 * spacing).abs() < 1e-3 * spacing,
                    "spacing {spacing} base {base}: {d}"
                );
            }
        }
    }

    fn units_of(linear: crate::native_scene::LinearUnit) -> Units {
        Units {
            linear,
            ..Units::default()
        }
    }

    #[test]
    fn the_drawn_spacing_is_the_line_spacing_in_the_scene_linear_unit() {
        use crate::native_scene::LinearUnit;
        let cases = [
            (LinearUnit::Centimeter, 1.0),
            (LinearUnit::Millimeter, 0.1),
            (LinearUnit::Meter, 100.0),
            (LinearUnit::Inch, 2.54),
            (LinearUnit::Foot, 30.48),
        ];
        for (linear, cm_per_unit) in cases {
            let got = drawn_grid_spacing_cm(DEFAULT_GRID_OPTIONS, &units_of(linear.clone()));
            assert_eq!(got.unwrap(), cm_per_unit, "{linear:?}");
        }
        let unsupported = units_of(LinearUnit::Unsupported("yard".into()));
        assert!(drawn_grid_spacing_cm(DEFAULT_GRID_OPTIONS, &unsupported).is_err());
    }

    #[test]
    fn the_grid_options_scale_the_drawn_spacing() {
        use crate::native_scene::LinearUnit;
        let custom = GridOptions {
            lines_every: 10.0,
            subdivisions: 4,
        };
        assert_eq!(custom.line_spacing_units(), 2.5);
        let inch = units_of(LinearUnit::Inch);
        assert_eq!(drawn_grid_spacing_cm(custom, &inch).unwrap(), 2.5 * 2.54);
    }

    #[test]
    fn spacing_scales_the_lines() {
        let d = snap_distance_to_grid(DVec3::ZERO, DVec3::Z, 7.0, 5.0).unwrap();
        assert_eq!(d, 5.0);
    }

    #[test]
    fn off_axis_and_degenerate_directions_are_refused() {
        let diagonal = DVec3::new(1.0, 1.0, 0.0).normalize();
        for direction in [diagonal, DVec3::ZERO, DVec3::new(0.999, 0.0, 0.04)] {
            assert_eq!(
                snap_distance_to_grid(DVec3::ZERO, direction, 1.0, 1.0),
                Err(SnapError::AxisOffGrid)
            );
        }
    }

    #[test]
    fn rounding_error_in_a_rotated_axis_still_counts_as_aligned() {
        let rotated = DVec3::new(6.1e-17, 1.0, -1e-12);
        assert!(snap_distance_to_grid(DVec3::ZERO, rotated, 2.4, 1.0).is_ok());
    }

    #[test]
    fn non_finite_and_bad_spacing_are_refused() {
        for (o, d, dist, s) in [
            (DVec3::NAN, DVec3::X, 1.0, 1.0),
            (DVec3::ZERO, DVec3::X, f64::INFINITY, 1.0),
            (DVec3::ZERO, DVec3::X, 1.0, 0.0),
            (DVec3::ZERO, DVec3::X, 1.0, -1.0),
            (DVec3::ZERO, DVec3::X, 1.0, f64::NAN),
        ] {
            assert_eq!(
                snap_distance_to_grid(o, d, dist, s),
                Err(SnapError::NonFinite)
            );
        }
        // The rounded line overflows when the pivot is near f64::MAX.
        assert_eq!(
            snap_distance_to_grid(DVec3::new(f64::MAX, 0.0, 0.0), DVec3::X, f64::MAX, 1.0),
            Err(SnapError::NonFinite)
        );
    }
}
