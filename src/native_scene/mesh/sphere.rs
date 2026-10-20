use super::{NativeMesh, axis::orient, grid::RevolveGrid};
use std::f64::consts::PI;

impl NativeMesh {
    /// UV sphere with poles on `axis`: a vertex at each pole, `height_divisions - 1` rings
    /// between them, triangle fans at the poles and outward-wound quads between rings.
    pub(super) fn polygon_sphere(grid: &RevolveGrid) -> Self {
        let RevolveGrid {
            radius,
            axis_divisions,
            height_divisions,
            axis,
        } = *grid;
        let mut points = vec![orient(axis, [0.0, radius as f32, 0.0])];
        for ring in 1..height_divisions {
            let polar = PI * ring as f64 / height_divisions as f64;
            let y = if 2 * ring == height_divisions {
                0.0
            } else {
                radius * polar.cos()
            };
            for column in 0..axis_divisions {
                let azimuth = 2.0 * PI * column as f64 / axis_divisions as f64;
                let radial = radius * polar.sin();
                points.push(orient(
                    axis,
                    [
                        (radial * azimuth.cos()) as f32,
                        y as f32,
                        (radial * azimuth.sin()) as f32,
                    ],
                ));
            }
        }
        points.push(orient(axis, [0.0, -radius as f32, 0.0]));
        let top = 0;
        let bottom = points.len() - 1;
        let vertex =
            |ring: usize, column: usize| 1 + (ring - 1) * axis_divisions + column % axis_divisions;
        let mut loops: Vec<Vec<usize>> = Vec::new();
        for column in 0..axis_divisions {
            loops.push(vec![top, vertex(1, column + 1), vertex(1, column)]);
        }
        for ring in 1..height_divisions - 1 {
            for column in 0..axis_divisions {
                loops.push(vec![
                    vertex(ring, column),
                    vertex(ring, column + 1),
                    vertex(ring + 1, column + 1),
                    vertex(ring + 1, column),
                ]);
            }
        }
        for column in 0..axis_divisions {
            loops.push(vec![
                bottom,
                vertex(height_divisions - 1, column),
                vertex(height_divisions - 1, column + 1),
            ]);
        }
        Self::from_loops(points, loops)
    }
}
