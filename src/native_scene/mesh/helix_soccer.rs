use super::{NativeMesh, axis::orient};
use crate::primitive_options::Axis;

/// Cyclic permutations of the three families below, with every sign choice, are the 60 vertices
/// of the truncated icosahedron with edge length 2 (`phi` is the golden ratio). Zero coordinates
/// make some sign choices coincide; those are kept once.
pub(super) fn soccer_ball_unit_points() -> Vec<[f64; 3]> {
    let phi = (1.0 + 5.0_f64.sqrt()) / 2.0;
    let families = [
        [0.0, 1.0, 3.0 * phi],
        [1.0, 2.0 + phi, 2.0 * phi],
        [phi, 2.0, 2.0 * phi + 1.0],
    ];
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(60);
    for family in families {
        for shift in 0..3 {
            let base = [
                family[shift],
                family[(shift + 1) % 3],
                family[(shift + 2) % 3],
            ];
            for signs in 0..8_u32 {
                let p = [0, 1, 2].map(|k| {
                    let sign = if (signs >> (2 - k)) & 1 == 0 {
                        1.0
                    } else {
                        -1.0
                    };
                    base[k] * sign
                });
                if !points.contains(&p) {
                    points.push(p);
                }
            }
        }
    }
    points
}

/// The 12 pentagons then the 20 hexagons as indices into `soccer_ball_unit_points`, each loop
/// counter-clockwise seen from outside. Derived once from the coordinates (a face is the vertex
/// set that maximises a dodecahedron or icosahedron direction) and pinned by the tests.
pub(super) const SOCCER_BALL_FACES: [&[usize]; 32] = [
    &[16, 40, 0, 36, 12],
    &[30, 54, 8, 52, 28],
    &[21, 45, 4, 44, 20],
    &[13, 37, 1, 41, 17],
    &[32, 56, 10, 58, 34],
    &[22, 46, 5, 47, 23],
    &[14, 38, 2, 42, 18],
    &[29, 53, 9, 55, 31],
    &[24, 48, 6, 49, 25],
    &[19, 43, 3, 39, 15],
    &[35, 59, 11, 57, 33],
    &[27, 51, 7, 50, 26],
    &[52, 20, 44, 12, 36, 28],
    &[29, 37, 13, 45, 21, 53],
    &[30, 38, 14, 46, 22, 54],
    &[55, 23, 47, 15, 39, 31],
    &[32, 40, 16, 48, 24, 56],
    &[57, 25, 49, 17, 41, 33],
    &[58, 26, 50, 18, 42, 34],
    &[35, 43, 19, 51, 27, 59],
    &[12, 44, 4, 6, 48, 16],
    &[28, 36, 0, 2, 38, 30],
    &[20, 52, 8, 9, 53, 21],
    &[17, 49, 6, 4, 45, 13],
    &[34, 42, 2, 0, 40, 32],
    &[23, 55, 9, 8, 54, 22],
    &[18, 50, 7, 5, 46, 14],
    &[31, 39, 3, 1, 37, 29],
    &[25, 57, 11, 10, 56, 24],
    &[15, 47, 5, 7, 51, 19],
    &[33, 41, 1, 3, 43, 35],
    &[26, 58, 10, 11, 59, 27],
];

impl NativeMesh {
    /// Soccer ball: a truncated icosahedron (12 pentagons, 20 hexagons, 60 vertices, 90 edges)
    /// centred on the origin, every edge `side_length` long. PROVISIONAL (#5): the size meaning
    /// (`-sideLength` as the edge), the orientation, point order and winding versus Maya are
    /// unverified.
    pub(super) fn soccer_ball(side_length: f64, axis: Axis) -> Self {
        let scale = side_length / 2.0;
        let points = soccer_ball_unit_points()
            .iter()
            .map(|p| orient(axis, p.map(|c| (c * scale) as f32)))
            .collect();
        let loops = SOCCER_BALL_FACES.iter().map(|face| face.to_vec()).collect();
        Self::from_loops(points, loops)
    }

    /// Helix: a closed tube of `sub_axis` points per ring and `sub_coil + 1` rings, swept along a
    /// helix of `coils` turns whose centreline has diameter `width` and spans `height` along the
    /// axis, centred on the origin, turning counter-clockwise seen from +Y. The two end rings are
    /// capped by one n-gon each, so it is a closed genus-0 mesh (V-E+F=2). PROVISIONAL (#5): what
    /// width and height measure, per-coil versus total subdivisions, the start angle, the point
    /// order and the winding are unverified against Maya; the pages list no axis flag, direction
    /// is not built, and round or subdivided caps are not built. A tube radius above the helix
    /// radius or the pitch self-intersects; nothing refuses it.
    pub(super) fn helix(
        coils: f64,
        height: f64,
        width: f64,
        radius: f64,
        sub_axis: u32,
        sub_coil: u32,
        axis: Axis,
    ) -> Self {
        let (n, segments) = (sub_axis as usize, sub_coil as usize);
        let helix_radius = width / 2.0;
        let total_angle = coils * std::f64::consts::TAU;
        let mut points = Vec::with_capacity(n * (segments + 1));
        for k in 0..=segments {
            let t = k as f64 / segments as f64;
            let theta = total_angle * t;
            let centre = [
                helix_radius * theta.cos(),
                height * (t - 0.5),
                -helix_radius * theta.sin(),
            ];
            let radial = [theta.cos(), 0.0, -theta.sin()];
            let tangent = unit([
                -helix_radius * theta.sin() * total_angle,
                height,
                -helix_radius * theta.cos() * total_angle,
            ]);
            // (radial, binormal, tangent) is right-handed, so increasing `phi` runs
            // counter-clockwise about the tangent and each side quad faces outwards.
            let binormal = cross(tangent, radial);
            for j in 0..n {
                let phi = std::f64::consts::TAU * j as f64 / n as f64;
                points.push([0, 1, 2].map(|c| {
                    centre[c] + radius * (phi.cos() * radial[c] + phi.sin() * binormal[c])
                }));
            }
        }
        let mut loops: Vec<Vec<usize>> = vec![(0..n).rev().collect()];
        for k in 0..segments {
            for j in 0..n {
                let next = (j + 1) % n;
                loops.push(vec![
                    k * n + j,
                    k * n + next,
                    (k + 1) * n + next,
                    (k + 1) * n + j,
                ]);
            }
        }
        loops.push((segments * n..(segments + 1) * n).collect());
        let points = points
            .iter()
            .map(|p| orient(axis, p.map(|c| c as f32)))
            .collect();
        Self::from_loops(points, loops)
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / length)
}
