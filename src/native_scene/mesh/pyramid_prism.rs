use super::{NativeMesh, axis::orient};
use crate::primitive_options::Axis;

/// Circumradius of a regular `sides`-gon with edge `side_length`. Maya's `-sideLength` is the
/// edge of the base polygon (command pages, 2027).
fn circumradius(side_length: f64, sides: u32) -> f64 {
    side_length / (2.0 * (std::f64::consts::PI / f64::from(sides)).sin())
}

/// Ring of `sides` points at height `y` around +Y, counter-clockwise seen from +Y. The first
/// point lies on +X; PROVISIONAL (#5): Maya's start angle is not documented.
fn ring(sides: u32, radius: f64, y: f64) -> Vec<[f64; 3]> {
    (0..sides)
        .map(|i| {
            let angle = std::f64::consts::TAU * f64::from(i) / f64::from(sides);
            [radius * angle.cos(), y, -radius * angle.sin()]
        })
        .collect()
}

fn to_points(axis: Axis, points: &[[f64; 3]]) -> Vec<[f32; 3]> {
    points
        .iter()
        .map(|p| orient(axis, p.map(|c| c as f32)))
        .collect()
}

impl NativeMesh {
    /// Pyramid: an n-gon base facing -Y and `sides` triangles meeting at the apex. Centred on
    /// the origin along the axis. PROVISIONAL (#5): the height is not documented and equals
    /// `side_length`; the centring, point order and winding are unverified against Maya.
    pub(super) fn polygon_pyramid(side_length: f64, sides: u32, axis: Axis) -> Self {
        let n = sides as usize;
        let height = side_length;
        let mut points = ring(sides, circumradius(side_length, sides), -height / 2.0);
        points.push([0.0, height / 2.0, 0.0]);
        let mut loops: Vec<Vec<usize>> = vec![(0..n).rev().collect()];
        loops.extend((0..n).map(|i| vec![i, (i + 1) % n, n]));
        Self::from_loops(to_points(axis, &points), loops)
    }

    /// Prism: two n-gon caps joined by `sides` quads, `length` along the axis and centred on the
    /// origin. PROVISIONAL (#5): the centring, start angle, point order and winding are
    /// unverified against Maya.
    pub(super) fn polygon_prism(side_length: f64, length: f64, sides: u32, axis: Axis) -> Self {
        let n = sides as usize;
        let radius = circumradius(side_length, sides);
        let mut points = ring(sides, radius, -length / 2.0);
        points.extend(ring(sides, radius, length / 2.0));
        let mut loops: Vec<Vec<usize>> = vec![(0..n).rev().collect(), (n..2 * n).collect()];
        loops.extend((0..n).map(|i| vec![i, (i + 1) % n, n + (i + 1) % n, n + i]));
        Self::from_loops(to_points(axis, &points), loops)
    }
}
