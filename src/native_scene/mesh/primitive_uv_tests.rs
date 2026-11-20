use super::*;
use crate::native_scene::NativeScene;
use crate::scene::Primitive;
use std::f64::consts::PI;

const AXES: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];

fn sphere(axis: Axis, c: u32, h: u32) -> PrimitiveOptions {
    PrimitiveOptions::Sphere {
        radius: 2.0,
        sub_axis: c,
        sub_height: h,
        axis,
    }
}

fn solid(
    cone: bool,
    axis: Axis,
    (c, b, caps): (u32, u32, u32),
    round_cap: bool,
) -> PrimitiveOptions {
    let (radius, height) = (1.5, 3.0);
    if cone {
        PrimitiveOptions::Cone {
            radius,
            height,
            sub_axis: c,
            sub_height: b,
            sub_caps: caps,
            round_cap,
            axis,
        }
    } else {
        PrimitiveOptions::Cylinder {
            radius,
            height,
            sub_axis: c,
            sub_height: b,
            sub_caps: caps,
            round_cap,
            axis,
        }
    }
}

fn torus(axis: Axis, c: u32, s: u32, twist: f64) -> PrimitiveOptions {
    PrimitiveOptions::Torus {
        radius: 2.0,
        section_radius: 0.5,
        twist,
        sub_axis: c,
        sub_height: s,
        axis,
    }
}

/// Every option combination the create path allows that these tests walk.
fn all_options() -> Vec<PrimitiveOptions> {
    let mut all = Vec::new();
    for axis in AXES {
        all.push(sphere(axis, 3, 2));
        all.push(sphere(axis, 8, 5));
        for cone in [false, true] {
            for caps in [0, 1, 2] {
                for round in [false, true] {
                    all.push(solid(cone, axis, (3, 1, caps), round));
                    all.push(solid(cone, axis, (8, 3, caps), round));
                }
            }
        }
        for twist in [0.0, 37.0, 360.0] {
            all.push(torus(axis, 3, 3, twist));
            all.push(torus(axis, 8, 5, twist));
        }
    }
    all
}

fn built(options: &PrimitiveOptions) -> NativeScene {
    let mut scene = NativeScene::default();
    scene
        .create_with_options(options.primitive(), options, "p", true)
        .unwrap();
    scene
}

fn corners(scene: &NativeScene) -> Vec<Vec<([f64; 3], [f64; 2])>> {
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
                    let at = mesh.effective_point_cm(mesh.directed_start(*d).unwrap());
                    (at.unwrap(), *uv)
                })
                .collect()
        })
        .collect()
}

fn area(face: &[([f64; 3], [f64; 2])]) -> f64 {
    (0..face.len())
        .map(|i| {
            let (a, b) = (face[i].1, face[(i + 1) % face.len()].1);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        / 2.0
}

#[test]
fn every_primitive_and_option_combination_validates_in_range_with_no_degenerate_face() {
    for options in all_options() {
        let scene = built(&options);
        scene.validate().unwrap();
        let shape = &scene.shapes()[0];
        let set = scene.uv_set(shape.id()).unwrap();
        assert_eq!(set.faces().len(), shape.mesh().faces().len(), "{options:?}");
        for face in corners(&scene) {
            assert!(
                area(&face) > 1e-9,
                "{options:?}: mirrored or degenerate UV face {face:?}"
            );
            for (_, [u, v]) in face {
                assert!(u.is_finite() && v.is_finite(), "{options:?}");
                assert!(
                    (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v),
                    "{options:?} {u} {v}"
                );
            }
        }
    }
}

/// Right-handed azimuth about the axis, in turns: `u = psi / 2pi + 3/4` modulo 1 (PROVISIONAL).
fn expected_u(axis: Axis, p: [f64; 3]) -> f64 {
    let [x, _, z] = match axis {
        Axis::Y => p,
        Axis::X => [-p[1], p[0], p[2]],
        Axis::Z => [p[0], p[2], -p[1]],
    };
    (x.atan2(z) / (2.0 * PI) + 0.75).rem_euclid(1.0)
}

fn turns_apart(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(1.0);
    d.min(1.0 - d)
}

fn axis_of(options: &PrimitiveOptions) -> Axis {
    match *options {
        PrimitiveOptions::Sphere { axis, .. }
        | PrimitiveOptions::Cylinder { axis, .. }
        | PrimitiveOptions::Cone { axis, .. }
        | PrimitiveOptions::Torus { axis, .. } => axis,
        _ => unreachable!("layouts cover only the revolved primitives"),
    }
}

/// Faces before the caps: all of them for a sphere or torus, else the side bands (a cone's apex
/// fan plus its bands is also `columns * bands`).
fn side_face_count(options: &PrimitiveOptions) -> usize {
    match *options {
        PrimitiveOptions::Cylinder {
            sub_axis,
            sub_height,
            ..
        }
        | PrimitiveOptions::Cone {
            sub_axis,
            sub_height,
            ..
        } => (sub_axis * sub_height) as usize,
        _ => usize::MAX,
    }
}

#[test]
fn u_follows_each_corners_mesh_azimuth_on_every_face_and_v_follows_the_height() {
    for options in all_options() {
        let axis = axis_of(&options);
        let scene = built(&options);
        let up = |p: [f64; 3]| match axis {
            Axis::Y => p[1],
            Axis::X => p[0],
            Axis::Z => p[2],
        };
        let radial = |p: [f64; 3]| match axis {
            Axis::Y => p[0].hypot(p[2]),
            Axis::X => p[1].hypot(p[2]),
            Axis::Z => p[0].hypot(p[1]),
        };
        let mut checked = 0;
        for face in corners(&scene).iter().take(side_face_count(&options)) {
            for (p, [u, _]) in face {
                // A pole or apex corner sits on the axis and takes its column's midpoint instead.
                if radial(*p) > 1e-6 {
                    assert!(
                        turns_apart(*u, expected_u(axis, *p)) < 1e-6,
                        "{options:?}: corner {p:?} has u {u}"
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "{options:?}");
        // v rises with height along a cylinder side: higher points never have a lower v.
        if let PrimitiveOptions::Cylinder { .. } = options {
            for face in corners(&scene).iter().take(side_face_count(&options)) {
                for (pa, [_, va]) in face {
                    for (pb, [_, vb]) in face {
                        if up(*pa) > up(*pb) + 1e-9 {
                            assert!(va > vb, "{options:?}: v must rise with height");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn caps_and_side_strip_occupy_separate_halves_of_the_square() {
    for options in all_options() {
        let sides = side_face_count(&options);
        let (cap_faces, top_cap) = match options {
            PrimitiveOptions::Cylinder {
                sub_axis, sub_caps, ..
            } => {
                let n = if sub_caps == 0 {
                    1
                } else {
                    (sub_axis * sub_caps) as usize
                };
                (n, true)
            }
            PrimitiveOptions::Cone {
                sub_axis, sub_caps, ..
            } => (
                if sub_caps == 0 {
                    1
                } else {
                    (sub_axis * sub_caps) as usize
                },
                false,
            ),
            _ => continue,
        };
        let scene = built(&options);
        for (index, face) in corners(&scene).iter().enumerate() {
            for (_, [u, v]) in face {
                if index < sides {
                    assert!(*v <= 0.5, "{options:?}: side corner v {v}");
                } else {
                    assert!(
                        *v >= 0.5,
                        "{options:?}: cap corner v {v} overlaps the strip"
                    );
                    // A cylinder's first cap is the top (left half); the rest are bottom caps.
                    let top = top_cap && index < sides + cap_faces;
                    // The rim of a disc touches the middle line, so each half is closed.
                    let in_half = if top { *u <= 0.5 } else { *u >= 0.5 };
                    assert!(in_half, "{options:?}: cap corner u {u} (top cap: {top})");
                }
            }
        }
    }
}

/// How far the apex sits above its base corners in v: one latitude step, one strip band.
fn apex_height(options: &PrimitiveOptions) -> f64 {
    match *options {
        PrimitiveOptions::Sphere { sub_height, .. } => 1.0 / f64::from(sub_height),
        PrimitiveOptions::Cone { sub_height, .. } => 0.5 / f64::from(sub_height),
        _ => unreachable!("only a sphere and a cone have an apex"),
    }
}

#[test]
fn sphere_poles_and_the_cone_apex_are_sawtooth() {
    for (options, pole_corner, faces) in [
        (sphere(Axis::Y, 8, 4), 0usize, 8usize),
        (solid(true, Axis::Y, (8, 2, 0), false), 0, 8),
    ] {
        let scene = built(&options);
        let fans: Vec<_> = corners(&scene).into_iter().take(faces).collect();
        let mut apex_us: Vec<u64> = fans
            .iter()
            .map(|face| {
                assert_eq!(face.len(), 3);
                assert!(area(face) > 1e-9, "{options:?}: stretched pole face");
                let apex = face[pole_corner].1;
                let base = [face[1].1, face[2].1];
                // The apex sits strictly between its base corners in u and above them in v.
                let (lo, hi) = (base[0][0].min(base[1][0]), base[0][0].max(base[1][0]));
                assert!(lo < apex[0] && apex[0] < hi, "{options:?}: apex u {apex:?}");
                assert!(apex[1] > base[0][1], "{options:?}: apex v {apex:?}");
                assert!(
                    (apex[1] - base[0][1] - apex_height(&options)).abs() < 1e-6,
                    "{options:?}"
                );
                apex[0].to_bits()
            })
            .collect();
        apex_us.sort_unstable();
        apex_us.dedup();
        assert_eq!(apex_us.len(), faces, "{options:?}: one apex UV per column");
    }
    // The bottom sphere pole is sawtooth too.
    let scene = built(&sphere(Axis::Y, 8, 4));
    let all = corners(&scene);
    let mut bottoms: Vec<u64> = all[all.len() - 8..]
        .iter()
        .map(|f| f[0].1[0].to_bits())
        .collect();
    bottoms.sort_unstable();
    bottoms.dedup();
    assert_eq!(bottoms.len(), 8);
}

#[test]
fn the_u_wrap_is_per_face_and_never_spans_the_whole_range() {
    for options in [
        sphere(Axis::Y, 8, 4),
        solid(false, Axis::Z, (8, 2, 0), false),
        solid(true, Axis::X, (8, 2, 0), false),
        torus(Axis::Y, 8, 5, 0.0),
    ] {
        let scene = built(&options);
        let side_faces = match options {
            PrimitiveOptions::Sphere { .. } | PrimitiveOptions::Torus { .. } => usize::MAX,
            _ => 16,
        };
        let (mut at_one, mut at_zero) = (false, false);
        for face in corners(&scene).iter().take(side_faces) {
            let us: Vec<f64> = face.iter().map(|c| c.1[0]).collect();
            let span = us.iter().cloned().fold(f64::MIN, f64::max)
                - us.iter().cloned().fold(f64::MAX, f64::min);
            assert!(span <= 1.0 / 8.0 + 1e-6, "{options:?} spans {span}");
            at_one |= us.contains(&1.0);
            at_zero |= us.contains(&0.0);
        }
        assert!(
            at_one && at_zero,
            "{options:?}: the seam reaches exactly 0 and 1"
        );
    }
}

#[test]
fn the_torus_wraps_v_to_exactly_one_without_a_wide_face() {
    let scene = built(&torus(Axis::Y, 4, 6, 37.0));
    let mut top = false;
    for face in corners(&scene) {
        let vs: Vec<f64> = face.iter().map(|c| c.1[1]).collect();
        let span = vs.iter().cloned().fold(f64::MIN, f64::max)
            - vs.iter().cloned().fold(f64::MAX, f64::min);
        assert!(span <= 1.0 / 6.0 + 1e-6, "v span {span}");
        top |= vs.contains(&1.0);
    }
    assert!(top);
}

#[test]
fn a_flat_plane_and_the_unsupported_primitives_are_handled_at_the_entry() {
    let mesh = NativeMesh::primitive(Primitive::PlatonicSolid).unwrap();
    let platonic = PrimitiveOptions::default_for(Primitive::PlatonicSolid);
    assert_eq!(
        layout(&mesh, &platonic).err(),
        Some(NativeError::UnsupportedUvTopology)
    );
    for axis in AXES {
        let plane = PrimitiveOptions::Plane {
            width: 2.0,
            height: 3.0,
            sub_width: 2,
            sub_height: 3,
            axis,
        };
        let scene = built(&plane);
        for face in corners(&scene) {
            assert!(area(&face) > 1e-9, "{axis:?}");
        }
    }
}

#[test]
fn pole_and_apex_corners_sit_at_the_middle_of_their_column() {
    for options in all_options() {
        let (columns, apex_faces, bottom_pole) = match options {
            PrimitiveOptions::Sphere { sub_axis, .. } => {
                (sub_axis as usize, 0..sub_axis as usize, true)
            }
            PrimitiveOptions::Cone { sub_axis, .. } => {
                (sub_axis as usize, 0..sub_axis as usize, false)
            }
            _ => continue,
        };
        let faces = corners(&built(&options));
        let middle = |column: usize| (columns as f64 - column as f64 - 0.5) / columns as f64;
        for column in apex_faces {
            let u = faces[column][0].1[0];
            assert!((u - middle(column)).abs() < 1e-6, "{options:?}: top u {u}");
        }
        if bottom_pole {
            for column in 0..columns {
                let u = faces[faces.len() - columns + column][0].1[0];
                assert!(
                    (u - middle(column)).abs() < 1e-6,
                    "{options:?}: bottom u {u}"
                );
            }
        }
    }
}

#[test]
fn platonic_solids_get_the_planar_placeholder_and_validate() {
    // NOT DONE: no layout is designed for them; they share the cube's Y planar placeholder.
    for solid in crate::primitive_options::PlatonicSolidKind::ALL {
        for axis in AXES {
            let options = PrimitiveOptions::PlatonicSolid {
                solid,
                radius: 1.5,
                axis,
            };
            let scene = built(&options);
            scene.validate().unwrap();
            let shape = &scene.shapes()[0];
            let set = scene.uv_set(shape.id()).expect("uvs");
            assert_eq!(set.faces().len(), shape.mesh().faces().len(), "{options:?}");
            for corners in set.faces() {
                for [u, v] in corners {
                    assert!(
                        (0.0..=1.0).contains(u) && (0.0..=1.0).contains(v),
                        "{options:?}"
                    );
                }
            }
        }
    }
}
