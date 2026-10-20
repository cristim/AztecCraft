use super::{NativeMesh, axis::orient, grid::RevolveGrid};
use std::f64::consts::PI;

impl NativeMesh {
    /// Cone along `axis`: the apex (vertex 0) on top, `height_divisions` rings of
    /// `axis_divisions` vertices below it (the last is the base), an outward triangle fan, quad
    /// bands between rings and one true n-gon base cap, with no cap centre vertex.
    pub(super) fn polygon_cone(grid: &RevolveGrid, height: f64) -> Self {
        let RevolveGrid {
            radius,
            axis_divisions: columns,
            height_divisions: bands,
            axis,
        } = *grid;
        let mut points = vec![orient(axis, [0.0, (0.5 * height) as f32, 0.0])];
        for ring in 1..=bands {
            let fraction = ring as f64 / bands as f64;
            let y = (0.5 - fraction) * height;
            for column in 0..columns {
                let azimuth = 2.0 * PI * column as f64 / columns as f64;
                let ring_radius = radius * fraction;
                points.push(orient(
                    axis,
                    [
                        (ring_radius * azimuth.cos()) as f32,
                        y as f32,
                        (ring_radius * azimuth.sin()) as f32,
                    ],
                ));
            }
        }
        let vertex = |ring: usize, column: usize| 1 + (ring - 1) * columns + column % columns;
        let mut loops: Vec<Vec<usize>> = Vec::new();
        for column in 0..columns {
            loops.push(vec![0, vertex(1, column + 1), vertex(1, column)]);
        }
        for ring in 1..bands {
            for column in 0..columns {
                loops.push(vec![
                    vertex(ring, column),
                    vertex(ring, column + 1),
                    vertex(ring + 1, column + 1),
                    vertex(ring + 1, column),
                ]);
            }
        }
        loops.push((0..columns).map(|c| vertex(bands, c)).collect());
        Self::from_loops(points, loops)
    }
}
