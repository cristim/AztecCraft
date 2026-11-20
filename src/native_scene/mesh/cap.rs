//! Subdivided and round caps shared by the cylinder and the cone (#453).
//!
//! Maya's 2027 options page (GUID-45D2EAD4) calls cap divisions "the number of subdivisions
//! around the origin of the primitive caps" and says the value "must be one or greater to see the
//! round cap effect", so `round_cap` at 0 subdivisions changing nothing is documented.
//!
//! PROVISIONAL: everything else is an AztecCraft choice, namely the ring spacing, the linear
//! shrink and the dome formula and extent. Cap ring `k` runs from the rim (`k = 0`, the existing
//! side ring) to the centre (`k = sub_caps`). A flat cap shrinks its radius linearly,
//! `r_k = R (1 - k/N)`. A round cap sits on a quarter circle: `t_k = 90 deg * k/N`,
//! `r_k = R cos t_k`, `h_k = R sin t_k` beyond the cap plane, so the centre is `R` beyond it.

use super::axis::orient;
use crate::primitive_options::Axis;
use std::f64::consts::{FRAC_PI_2, PI};

/// Cap options already validated by `PrimitiveOptions::validate`.
#[derive(Clone, Copy)]
pub(super) struct CapOptions {
    pub sub_caps: usize,
    pub round_cap: bool,
}

/// One cap: where its rim ring lies and which way it faces along the local up axis.
pub(super) struct Cap {
    pub radius: f64,
    pub columns: usize,
    pub axis: Axis,
    /// Local height of the rim ring.
    pub rim_y: f64,
    /// `1.0` for a cap facing +y (top), `-1.0` for one facing -y (bottom).
    pub facing: f64,
    pub options: CapOptions,
}

impl Cap {
    /// Radius and height beyond the rim plane of ring `k` (`1..=sub_caps`).
    fn ring(&self, k: usize) -> (f64, f64) {
        let n = self.options.sub_caps as f64;
        if self.options.round_cap {
            let t = FRAC_PI_2 * k as f64 / n;
            (self.radius * t.cos(), self.radius * t.sin())
        } else {
            (self.radius * (1.0 - k as f64 / n), 0.0)
        }
    }

    /// Appends the cap's new vertices to `points` and its faces to `loops`. `rim(column)` is the
    /// existing rim vertex. `descending` flips the winding for the cap whose outward side is
    /// reached by decreasing azimuth (the top cap).
    pub(super) fn append(
        &self,
        points: &mut Vec<[f32; 3]>,
        loops: &mut Vec<Vec<usize>>,
        rim: impl Fn(usize) -> usize,
        descending: bool,
    ) {
        let (columns, n) = (self.columns, self.options.sub_caps);
        let pick = |c: usize| if descending { columns - c } else { c };
        if n == 0 {
            loops.push((0..columns).map(|c| rim(pick(c))).collect());
            return;
        }
        let base = points.len();
        for k in 1..n {
            let (r, h) = self.ring(k);
            let y = (self.rim_y + self.facing * h) as f32;
            for column in 0..columns {
                let azimuth = 2.0 * PI * column as f64 / columns as f64;
                points.push(orient(
                    self.axis,
                    [(r * azimuth.cos()) as f32, y, (r * azimuth.sin()) as f32],
                ));
            }
        }
        let (_, centre_h) = self.ring(n);
        let centre_y = (self.rim_y + self.facing * centre_h) as f32;
        points.push(orient(self.axis, [0.0, centre_y, 0.0]));
        let centre = points.len() - 1;
        let vertex = |k: usize, column: usize| {
            if k == 0 {
                rim(column % columns)
            } else if k == n {
                centre
            } else {
                base + (k - 1) * columns + column % columns
            }
        };
        for k in 0..n {
            for column in 0..columns {
                let (a, b) = (column, column + 1);
                let (a, b) = if descending { (b, a) } else { (a, b) };
                if k + 1 == n {
                    loops.push(vec![vertex(k, a), vertex(k, b), centre]);
                } else {
                    loops.push(vec![
                        vertex(k, a),
                        vertex(k, b),
                        vertex(k + 1, b),
                        vertex(k + 1, a),
                    ]);
                }
            }
        }
    }
}
