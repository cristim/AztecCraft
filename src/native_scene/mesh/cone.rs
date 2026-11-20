use super::{
    NativeMesh,
    axis::orient,
    cap::{Cap, CapOptions, ring_point},
    grid::RevolveGrid,
};

impl NativeMesh {
    /// Cone along `axis`: the apex (vertex 0) on top, `height_divisions` rings of
    /// `axis_divisions` vertices below it (the last is the base), an outward triangle fan, quad
    /// bands between rings and one base cap: a true n-gon with no centre vertex at 0 cap subdivisions, else a centre vertex
    /// with a fan and quad bands, domed when `caps.round_cap` is set (see `cap`).
    pub(super) fn polygon_cone(grid: &RevolveGrid, height: f64, caps: CapOptions) -> Self {
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
            let ring_radius = radius * fraction;
            points.extend((0..columns).map(|c| ring_point(axis, ring_radius, y, c, columns)));
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
        Cap {
            radius,
            columns,
            axis,
            rim_y: -0.5 * height,
            facing: -1.0,
            options: caps,
        }
        .append(&mut points, &mut loops, |c| vertex(bands, c), false);
        Self::from_loops(points, loops)
    }
}
