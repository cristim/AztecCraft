use super::*;
use crate::native_scene::NativeScene;

const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

fn cube(axis: Axis, [sx, sy, sz]: [u32; 3]) -> PrimitiveOptions {
    PrimitiveOptions::Cube {
        width: 2.0,
        height: 3.0,
        depth: 5.0,
        sub_width: sx,
        sub_height: sy,
        sub_depth: sz,
        axis,
    }
}

fn all_cubes() -> Vec<PrimitiveOptions> {
    let mut all = Vec::new();
    for axis in AXES {
        for subs in [[1, 1, 1], [3, 2, 4], [2, 3, 1]] {
            all.push(cube(axis, subs));
        }
    }
    all
}

type Face = Vec<([f64; 3], [f64; 2])>;

/// Each face's corners as (position in the builder's +Y frame, stored uv).
fn faces(options: &PrimitiveOptions) -> Vec<Face> {
    let mut scene = NativeScene::default();
    scene
        .create_with_options(options.primitive(), options, "c", true)
        .unwrap();
    scene.validate().unwrap();
    let axis = match *options {
        PrimitiveOptions::Cube { axis, .. } => axis,
        _ => unreachable!(),
    };
    let shape = &scene.shapes()[0];
    let mesh = shape.mesh();
    let set = scene.uv_set(shape.id()).expect("uvs");
    mesh.faces()
        .iter()
        .zip(set.faces())
        .map(|(face, uvs)| {
            face.outer()
                .iter()
                .zip(uvs)
                .map(|(d, uv)| {
                    let p = mesh
                        .effective_point_cm(mesh.directed_start(*d).unwrap())
                        .unwrap();
                    (to_local(axis, p), *uv)
                })
                .collect()
        })
        .collect()
}

fn area(face: &Face) -> f64 {
    (0..face.len())
        .map(|i| {
            let (a, b) = (face[i].1, face[(i + 1) % face.len()].1);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

fn rect(face: &Face) -> [f64; 4] {
    let us = face.iter().map(|c| c.1[0]);
    let vs = face.iter().map(|c| c.1[1]);
    [
        us.clone().fold(f64::MAX, f64::min),
        vs.clone().fold(f64::MAX, f64::min),
        us.fold(f64::MIN, f64::max),
        vs.fold(f64::MIN, f64::max),
    ]
}

#[test]
fn every_cube_validates_in_range_with_positive_area_and_no_overlap() {
    for options in all_cubes() {
        let all = faces(&options);
        for face in &all {
            assert!(
                area(face) > 1e-9,
                "{options:?}: mirrored or degenerate face"
            );
            for (_, [u, v]) in face {
                assert!(u.is_finite() && v.is_finite());
                assert!(
                    (0.0..=1.0).contains(u) && (0.0..=1.0).contains(v),
                    "{options:?}"
                );
            }
        }
        let rects: Vec<[f64; 4]> = all.iter().map(rect).collect();
        for (i, a) in rects.iter().enumerate() {
            for (j, b) in rects.iter().enumerate().skip(i + 1) {
                let overlap = a[0] < b[2] - 1e-7
                    && b[0] < a[2] - 1e-7
                    && a[1] < b[3] - 1e-7
                    && b[1] < a[3] - 1e-7;
                assert!(!overlap, "{options:?}: faces {i} and {j} overlap");
            }
        }
    }
}

/// Side index of a face from its builder order: back, front, left, right, bottom, top.
fn side_of(index: usize, [nx, ny, nz]: [usize; 3]) -> usize {
    let counts = [nx * ny, nx * ny, nz * ny, nz * ny, nz * nx, nz * nx];
    let mut left = index;
    for (side, count) in counts.iter().enumerate() {
        if left < *count {
            return side;
        }
        left -= count;
    }
    unreachable!("face {index} is past the last side")
}

#[test]
fn every_corner_uv_follows_its_mesh_position_on_its_side() {
    for options in all_cubes() {
        let PrimitiveOptions::Cube {
            sub_width,
            sub_height,
            sub_depth,
            ..
        } = options
        else {
            unreachable!()
        };
        let subs = [sub_width, sub_height, sub_depth].map(|n| n as usize);
        let all = faces(&options);
        assert_eq!(
            all.len(),
            2 * (subs[0] * subs[1] + subs[1] * subs[2] + subs[0] * subs[2])
        );
        // Fractions of the box along each local axis, from the corner positions themselves.
        let low = [0, 1, 2].map(|c| {
            all.iter()
                .flatten()
                .map(|p| p.0[c])
                .fold(f64::MAX, f64::min)
        });
        let high = [0, 1, 2].map(|c| {
            all.iter()
                .flatten()
                .map(|p| p.0[c])
                .fold(f64::MIN, f64::max)
        });
        for (index, face) in all.iter().enumerate() {
            let side = side_of(index, subs);
            for (p, [u, v]) in face {
                let f = [0, 1, 2].map(|c| (p[c] - low[c]) / (high[c] - low[c]));
                let [s, t] = match side {
                    0 => [1.0 - f[0], f[1]],
                    1 => [f[0], f[1]],
                    2 => [f[2], f[1]],
                    3 => [1.0 - f[2], f[1]],
                    4 => [f[0], f[2]],
                    _ => [1.0 - f[0], f[2]],
                };
                let expected = [((side % 3) as f64 + s) / 3.0, ((side / 3) as f64 + t) / 2.0];
                assert!(
                    (u - expected[0]).abs() < 1e-6 && (v - expected[1]).abs() < 1e-6,
                    "{options:?} face {index} (side {side}): uv {u},{v} expected {expected:?}"
                );
            }
        }
    }
}

#[test]
fn each_side_fills_its_own_atlas_cell() {
    for options in all_cubes() {
        let PrimitiveOptions::Cube {
            sub_width,
            sub_height,
            sub_depth,
            ..
        } = options
        else {
            unreachable!()
        };
        let subs = [sub_width, sub_height, sub_depth].map(|n| n as usize);
        let mut cells = [[f64::MAX, f64::MAX, f64::MIN, f64::MIN]; 6];
        for (index, face) in faces(&options).iter().enumerate() {
            let r = rect(face);
            let c = &mut cells[side_of(index, subs)];
            *c = [
                c[0].min(r[0]),
                c[1].min(r[1]),
                c[2].max(r[2]),
                c[3].max(r[3]),
            ];
        }
        for (side, c) in cells.iter().enumerate() {
            let (col, row) = ((side % 3) as f64, (side / 3) as f64);
            let want = [col / 3.0, row / 2.0, (col + 1.0) / 3.0, (row + 1.0) / 2.0];
            for k in 0..4 {
                assert!(
                    (c[k] - want[k]).abs() < 1e-6,
                    "{options:?} side {side}: {c:?} vs {want:?}"
                );
            }
        }
    }
}
