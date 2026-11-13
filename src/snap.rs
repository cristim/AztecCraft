//! Snap to grids for the Move tool: the lead object's pivot lands on a grid line of the drawn
//! grid. Pure math over world centimetres; the editor feeds it the lead pivot and handle axis.

use glam::DVec3;
use std::fmt;

/// Distance between the drawn grid's lines (one minor line per render unit, which is one
/// centimetre; `viewport_renderer::GRID_SIZE`). PROVISIONAL: Maya's grid spacing is in scene
/// linear units; ours follows what is drawn.
pub const GRID_LINE_SPACING_CM: f64 = 1.0;

/// A handle axis within this of a world axis counts as aligned; an Object or Parent axis
/// rotated by a multiple of 90 degrees differs from it by rounding error only.
const AXIS_TOLERANCE: f64 = 1e-9;

/// A pivot within this fraction of max(|coordinate|, 1) of a grid line counts as on it: 32 times
/// 2^20 (the inverse of the core's 2^-20 move resolution, `MIN_RESOLUTION`) times f64 epsilon,
/// about 7.45e-9, so the stored channel's few-ulp rounding error stays well inside it.
/// Revisit when a grid spacing below 1 cm arrives (#48): the tolerance is relative to the
/// coordinate, not to the spacing.
const ON_LINE_RELATIVE: f64 = 32.0 * f64::EPSILON * 1_048_576.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapError {
    /// The Move axis is not along a world X, Y or Z grid axis, so no grid line lies on it.
    AxisOffGrid,
    /// An input or the snapped result is not a finite number.
    NonFinite,
}

impl fmt::Display for SnapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AxisOffGrid => f.write_str(
                "Snap to grids needs the Move axis along a world X, Y or Z grid axis; \
                 use World Axis Orientation or turn Snap to grids off",
            ),
            Self::NonFinite => f.write_str("Snap to grids needs finite values"),
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
    let result = if gap.abs() <= ON_LINE_RELATIVE * origin[index].abs().max(1.0) {
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

#[cfg(test)]
mod tests {
    use super::*;

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
