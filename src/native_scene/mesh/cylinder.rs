use super::{
    NativeMesh,
    axis::orient,
    cap::{Cap, CapOptions},
    grid::RevolveGrid,
};
use std::f64::consts::PI;

impl NativeMesh {
    /// Cylinder along `axis`: `height_divisions + 1` rings of `axis_divisions` vertices from the
    /// top (ring 0) down, outward side quads per band and two caps. A cap is one true n-gon with no
    /// centre vertex at 0 cap subdivisions, else a centre vertex with a fan and quad bands, domed
    /// when `caps.round_cap` is set (see `cap`).
    pub(super) fn polygon_cylinder(grid: &RevolveGrid, height: f64, caps: CapOptions) -> Self {
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
        let cap = |rim_y: f64, facing: f64| Cap {
            radius,
            columns,
            axis,
            rim_y,
            facing,
            options: caps,
        };
        cap(0.5 * height, 1.0).append(&mut points, &mut loops, |c| vertex(0, c), true);
        cap(-0.5 * height, -1.0).append(&mut points, &mut loops, |c| vertex(bands, c), false);
        Self::from_loops(points, loops)
    }
}
