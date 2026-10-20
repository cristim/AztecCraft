use super::{NativeMesh, axis::orient, grid::RevolveGrid};
use std::f64::consts::PI;

impl NativeMesh {
    /// Cylinder along `axis`: `height_divisions + 1` rings of `axis_divisions` vertices from the
    /// top (ring 0) down, outward side quads per band and two true n-gon caps, with no cap centre
    /// vertices.
    pub(super) fn polygon_cylinder(grid: &RevolveGrid, height: f64) -> Self {
        let RevolveGrid {
            radius,
            axis_divisions: columns,
            height_divisions: bands,
            axis,
        } = *grid;
        let mut points = Vec::with_capacity((bands + 1) * columns);
        for ring in 0..=bands {
            let y = (0.5 - ring as f64 / bands as f64) * height;
            for column in 0..columns {
                let azimuth = 2.0 * PI * column as f64 / columns as f64;
                points.push(orient(
                    axis,
                    [
                        (radius * azimuth.cos()) as f32,
                        y as f32,
                        (radius * azimuth.sin()) as f32,
                    ],
                ));
            }
        }
        let vertex = |ring: usize, column: usize| ring * columns + column % columns;
        let mut loops: Vec<Vec<usize>> = Vec::new();
        for band in 0..bands {
            for column in 0..columns {
                loops.push(vec![
                    vertex(band, column),
                    vertex(band, column + 1),
                    vertex(band + 1, column + 1),
                    vertex(band + 1, column),
                ]);
            }
        }
        loops.push((0..columns).map(|c| vertex(0, columns - c)).collect());
        loops.push((0..columns).map(|c| vertex(bands, c)).collect());
        Self::from_loops(points, loops)
    }
}
