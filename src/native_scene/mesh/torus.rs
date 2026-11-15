use super::{NativeMesh, axis::orient, grid::RevolveGrid};
use std::f64::consts::PI;

impl NativeMesh {
    /// Torus around `axis`: `height_divisions` section-circle points on each of
    /// `axis_divisions` rings (`radius` is the ring radius from the axis, `section_radius` the
    /// tube radius), joined by outward-wound quads with no poles and no caps (Euler
    /// characteristic 0), with every section turned by `twist_deg` about the tube centre
    /// (PROVISIONAL: Maya's formula is unstated). Vertex `ring * height_divisions + step`.
    pub(super) fn polygon_torus(grid: &RevolveGrid, section_radius: f64, twist_deg: f64) -> Self {
        let RevolveGrid {
            radius,
            axis_divisions: rings,
            height_divisions: steps,
            axis,
        } = *grid;
        // Every section turns by the same angle so the seam joins identical sections. The angle
        // is wrapped first: 360 is bit-identical to 0 and huge finite twists keep precision.
        let twist = twist_deg.rem_euclid(360.0).to_radians();
        let mut points = Vec::with_capacity(rings * steps);
        for ring in 0..rings {
            let azimuth = 2.0 * PI * ring as f64 / rings as f64;
            for step in 0..steps {
                let section = 2.0 * PI * step as f64 / steps as f64 + twist;
                let radial = radius + section_radius * section.cos();
                points.push(orient(
                    axis,
                    [
                        (radial * azimuth.cos()) as f32,
                        (section_radius * section.sin()) as f32,
                        (radial * azimuth.sin()) as f32,
                    ],
                ));
            }
        }
        let vertex = |ring: usize, step: usize| (ring % rings) * steps + step % steps;
        let mut loops = Vec::with_capacity(rings * steps);
        for ring in 0..rings {
            for step in 0..steps {
                loops.push(vec![
                    vertex(ring, step),
                    vertex(ring, step + 1),
                    vertex(ring + 1, step + 1),
                    vertex(ring + 1, step),
                ]);
            }
        }
        Self::from_loops(points, loops)
    }
}
