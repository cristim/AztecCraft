use super::{NativeMesh, axis::orient};
use crate::primitive_options::Axis;

/// Points of one `(na + 1) x (nb + 1)` lattice layer: the perimeter counter-clockwise from
/// `(0, 0)`, then the interior row by row. A 1x1 layer is the legacy four-corner ring.
fn layer(na: usize, nb: usize, full: bool) -> Vec<(usize, usize)> {
    let mut points: Vec<(usize, usize)> = (0..na).map(|a| (a, 0)).collect();
    points.extend((0..nb).map(|b| (na, b)));
    points.extend((1..=na).rev().map(|a| (a, nb)));
    points.extend((1..=nb).rev().map(|b| (0, b)));
    if full {
        for b in 1..nb {
            points.extend((1..na).map(|a| (a, b)));
        }
    }
    points
}

/// Position of `(a, b)` inside the layer built by `layer`.
fn layer_index(na: usize, nb: usize, a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else if a == na {
        na + b
    } else if b == nb {
        2 * na + nb - a
    } else if a == 0 {
        2 * na + 2 * nb - b
    } else {
        2 * (na + nb) + (b - 1) * (na - 1) + (a - 1)
    }
}

/// Fraction `i / n` of the way across a centred extent, so both ends are exact.
fn coord(i: usize, n: usize, size: f64) -> f32 {
    ((i as f64 / n as f64 - 0.5) * size) as f32
}

pub(super) struct CubeGrid {
    pub size: [f64; 3],
    pub divisions: [usize; 3],
    pub axis: Axis,
}

impl NativeMesh {
    /// Cube surface: layers along z (full grids at both ends, perimeters between), with faces
    /// back, front, left, right, bottom, top. At 1x1x1 this is the legacy cube.
    pub(super) fn cube_grid(grid: &CubeGrid) -> Self {
        let [nx, ny, nz] = grid.divisions;
        let [w, h, d] = grid.size;
        let mut points = Vec::new();
        for k in 0..=nz {
            for (i, j) in layer(nx, ny, k == 0 || k == nz) {
                points.push(orient(
                    grid.axis,
                    [coord(i, nx, w), coord(j, ny, h), coord(k, nz, d)],
                ));
            }
        }
        let end = (nx + 1) * (ny + 1);
        let ring = 2 * (nx + ny);
        let base = |k: usize| if k == 0 { 0 } else { end + (k - 1) * ring };
        let v = |i, j, k| base(k) + layer_index(nx, ny, i, j);
        let mut loops = Vec::new();
        for j in 0..ny {
            for i in 0..nx {
                loops.push(vec![
                    v(i, j, 0),
                    v(i, j + 1, 0),
                    v(i + 1, j + 1, 0),
                    v(i + 1, j, 0),
                ]);
            }
        }
        for j in 0..ny {
            for i in 0..nx {
                loops.push(vec![
                    v(i, j, nz),
                    v(i + 1, j, nz),
                    v(i + 1, j + 1, nz),
                    v(i, j + 1, nz),
                ]);
            }
        }
        for k in 0..nz {
            for j in 0..ny {
                loops.push(vec![
                    v(0, j, k),
                    v(0, j, k + 1),
                    v(0, j + 1, k + 1),
                    v(0, j + 1, k),
                ]);
            }
        }
        for k in 0..nz {
            for j in 0..ny {
                loops.push(vec![
                    v(nx, j, k),
                    v(nx, j + 1, k),
                    v(nx, j + 1, k + 1),
                    v(nx, j, k + 1),
                ]);
            }
        }
        for k in 0..nz {
            for i in 0..nx {
                loops.push(vec![
                    v(i, 0, k),
                    v(i + 1, 0, k),
                    v(i + 1, 0, k + 1),
                    v(i, 0, k + 1),
                ]);
            }
        }
        for k in 0..nz {
            for i in 0..nx {
                loops.push(vec![
                    v(i, ny, k),
                    v(i, ny, k + 1),
                    v(i + 1, ny, k + 1),
                    v(i + 1, ny, k),
                ]);
            }
        }
        Self::from_loops(points, loops)
    }

    /// `width` x `height` plane with `sub_width` x `sub_height` quads, normal +Y before `axis`.
    pub(super) fn plane_grid(size: [f64; 2], divisions: [usize; 2], axis: Axis) -> Self {
        let [nx, nz] = divisions;
        let points = layer(nz, nx, true)
            .into_iter()
            .map(|(k, i)| orient(axis, [coord(i, nx, size[0]), 0.0, coord(k, nz, size[1])]))
            .collect();
        let v = |k, i| layer_index(nz, nx, k, i);
        let mut loops = Vec::new();
        for i in 0..nx {
            for k in 0..nz {
                loops.push(vec![v(k, i), v(k + 1, i), v(k + 1, i + 1), v(k, i + 1)]);
            }
        }
        Self::from_loops(points, loops)
    }
}

/// Shared arguments of the sphere, cylinder and cone builders.
pub(super) struct RevolveGrid {
    pub radius: f64,
    pub axis_divisions: usize,
    pub height_divisions: usize,
    pub axis: Axis,
}
