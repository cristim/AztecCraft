use super::*;

const ALL: [Primitive; 5] = [
    Primitive::Cube,
    Primitive::Plane,
    Primitive::Sphere,
    Primitive::Cylinder,
    Primitive::Cone,
];

fn cube(width: f64, sub: u32) -> PrimitiveOptions {
    PrimitiveOptions::Cube {
        width,
        height: 1.0,
        depth: 1.0,
        sub_width: sub,
        sub_height: 1,
        sub_depth: 1,
        axis: Axis::Y,
    }
}

fn sphere(radius: f64, sub_axis: u32, sub_height: u32) -> PrimitiveOptions {
    PrimitiveOptions::Sphere {
        radius,
        sub_axis,
        sub_height,
        axis: Axis::Y,
    }
}

#[test]
fn defaults_match_the_documented_table() {
    for primitive in ALL {
        let options = PrimitiveOptions::default_for(primitive);
        assert_eq!(options.primitive(), primitive);
        assert_eq!(options.validate(primitive), Ok(()));
    }
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Sphere),
        sphere(1.0, 20, 20)
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cube),
        PrimitiveOptions::Cube {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
            sub_width: 1,
            sub_height: 1,
            sub_depth: 1,
            axis: Axis::Y
        }
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Plane),
        PrimitiveOptions::Plane {
            width: 1.0,
            height: 1.0,
            sub_width: 10,
            sub_height: 10,
            axis: Axis::Y
        }
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cylinder),
        PrimitiveOptions::Cylinder {
            radius: 1.0,
            height: 2.0,
            sub_axis: 20,
            sub_height: 1,
            axis: Axis::Y
        }
    );
    assert_eq!(Axis::default(), Axis::Y);
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cone),
        PrimitiveOptions::Cone {
            radius: 1.0,
            height: 2.0,
            sub_axis: 20,
            sub_height: 1,
            axis: Axis::Y
        }
    );
    assert_eq!(Axis::default(), Axis::Y);
}

#[test]
fn mismatched_variant_is_rejected() {
    for primitive in ALL {
        for other in ALL.into_iter().filter(|o| *o != primitive) {
            assert_eq!(
                PrimitiveOptions::default_for(other).validate(primitive),
                Err(PrimitiveOptionsError::PrimitiveMismatch {
                    expected: primitive
                })
            );
        }
    }
}

#[test]
fn size_range_boundaries() {
    let bad = |v| cube(v, 1).validate(Primitive::Cube);
    let err = Err(PrimitiveOptionsError::SizeOutOfRange { field: "width" });
    for v in [
        0.0,
        -0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        1e-7,
        MAX_SIZE_CM + 1.0,
    ] {
        assert_eq!(bad(v), err, "{v}");
    }
    for v in [MIN_SIZE_CM, 1.0, MAX_SIZE_CM] {
        assert_eq!(bad(v), Ok(()), "{v}");
    }
}

#[test]
fn division_range_boundaries() {
    let check = |o: PrimitiveOptions, p| o.validate(p);
    for (value, ok) in [
        (0, false),
        (1, true),
        (MAX_DIVISIONS, true),
        (MAX_DIVISIONS + 1, false),
        (u32::MAX, false),
    ] {
        assert_eq!(
            check(cube(1.0, value), Primitive::Cube).is_ok(),
            ok,
            "{value}"
        );
    }
    for (value, ok) in [
        (2, false),
        (3, true),
        (128, true),
        (129, false),
        (u32::MAX, false),
    ] {
        assert_eq!(
            check(sphere(1.0, value, 20), Primitive::Sphere).is_ok(),
            ok,
            "axis {value}"
        );
    }
    for (value, ok) in [(0, false), (1, false), (2, true), (128, true), (129, false)] {
        assert_eq!(
            check(sphere(1.0, 20, value), Primitive::Sphere).is_ok(),
            ok,
            "height {value}"
        );
    }
    for (bad_axis, bad_height) in [(2, 1), (3, 0)] {
        for make in [Primitive::Cylinder, Primitive::Cone] {
            let mut o = PrimitiveOptions::default_for(make);
            if let PrimitiveOptions::Cylinder {
                sub_axis,
                sub_height,
                ..
            }
            | PrimitiveOptions::Cone {
                sub_axis,
                sub_height,
                ..
            } = &mut o
            {
                *sub_axis = bad_axis;
                *sub_height = bad_height;
            }
            assert!(o.validate(make).is_err());
        }
    }
}

#[test]
fn face_counts_cover_caps_and_saturate() {
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cube).face_count(),
        6
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Plane).face_count(),
        100
    );
    assert_eq!(sphere(1.0, 8, 6).face_count(), 48);
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cylinder).face_count(),
        22
    );
    assert_eq!(
        PrimitiveOptions::default_for(Primitive::Cone).face_count(),
        21
    );
    let huge = PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: u32::MAX,
        sub_height: u32::MAX,
        sub_depth: u32::MAX,
        axis: Axis::Y,
    };
    assert_eq!(huge.face_count(), u64::MAX);
}

#[test]
fn field_ranges_reject_before_face_count_is_trusted() {
    let huge = PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: u32::MAX,
        sub_height: u32::MAX,
        sub_depth: u32::MAX,
        axis: Axis::Y,
    };
    assert!(matches!(
        huge.validate(Primitive::Cube),
        Err(PrimitiveOptionsError::DivisionsOutOfRange { .. })
    ));
}

#[test]
fn face_budget_rejects_the_largest_cube() {
    let biggest = PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: MAX_DIVISIONS,
        sub_height: MAX_DIVISIONS,
        sub_depth: MAX_DIVISIONS,
        axis: Axis::Y,
    };
    assert_eq!(biggest.face_count(), 98_304);
    assert_eq!(
        biggest.validate(Primitive::Cube),
        Err(PrimitiveOptionsError::TooManyFaces {
            faces: 98_304,
            budget: MAX_FACES
        })
    );
}

#[test]
fn face_budget_binds_and_admits_the_other_maxima() {
    const { assert!(MAX_FACES < 98_304) };
    for primitive in [
        Primitive::Plane,
        Primitive::Sphere,
        Primitive::Cylinder,
        Primitive::Cone,
    ] {
        let mut o = PrimitiveOptions::default_for(primitive);
        match &mut o {
            PrimitiveOptions::Plane {
                sub_width,
                sub_height,
                ..
            } => {
                *sub_width = MAX_DIVISIONS;
                *sub_height = MAX_DIVISIONS;
            }
            PrimitiveOptions::Sphere {
                sub_axis,
                sub_height,
                ..
            }
            | PrimitiveOptions::Cylinder {
                sub_axis,
                sub_height,
                ..
            }
            | PrimitiveOptions::Cone {
                sub_axis,
                sub_height,
                ..
            } => {
                *sub_axis = MAX_DIVISIONS;
                *sub_height = MAX_DIVISIONS;
            }
            PrimitiveOptions::Cube { .. } => unreachable!(),
        }
        assert_eq!(o.validate(primitive), Ok(()), "{primitive:?}");
    }
    let cube_of = |sub| PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: sub,
        sub_height: sub,
        sub_depth: sub,
        axis: Axis::Y,
    };
    assert_eq!(
        cube_of(58).validate(Primitive::Cube),
        Err(PrimitiveOptionsError::TooManyFaces {
            faces: 20_184,
            budget: MAX_FACES
        })
    );
}

fn cube_sub(w: u32, h: u32, d: u32) -> PrimitiveOptions {
    PrimitiveOptions::Cube {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
        sub_width: w,
        sub_height: h,
        sub_depth: d,
        axis: Axis::Y,
    }
}

fn round(primitive: Primitive, sub_axis: u32, sub_height: u32) -> PrimitiveOptions {
    match PrimitiveOptions::default_for(primitive) {
        PrimitiveOptions::Cylinder {
            radius,
            height,
            axis,
            ..
        } => PrimitiveOptions::Cylinder {
            radius,
            height,
            sub_axis,
            sub_height,
            axis,
        },
        PrimitiveOptions::Cone {
            radius,
            height,
            axis,
            ..
        } => PrimitiveOptions::Cone {
            radius,
            height,
            sub_axis,
            sub_height,
            axis,
        },
        other => other,
    }
}

#[test]
fn budget_accepts_57_and_rejects_58_subdivisions_per_axis() {
    assert_eq!(cube_sub(57, 57, 57).face_count(), 19_494);
    assert_eq!(cube_sub(57, 57, 57).validate(Primitive::Cube), Ok(()));
    assert!(matches!(
        cube_sub(58, 58, 58).validate(Primitive::Cube),
        Err(PrimitiveOptionsError::TooManyFaces { .. })
    ));
}

#[test]
fn budget_accepts_exactly_max_faces_and_rejects_one_quad_pair_over() {
    let at = cube_sub(50, 50, 75);
    assert_eq!(at.face_count(), MAX_FACES);
    assert_eq!(at.validate(Primitive::Cube), Ok(()));
    // Cube counts are even, so 20,002 is the nearest count above the budget.
    let over = cube_sub(2, 85, 113);
    assert_eq!(over.face_count(), MAX_FACES + 2);
    assert_eq!(
        over.validate(Primitive::Cube),
        Err(PrimitiveOptionsError::TooManyFaces {
            faces: MAX_FACES + 2,
            budget: MAX_FACES
        })
    );
}

#[test]
fn cube_face_count_distinguishes_axes() {
    // 2*(2*3 + 3*4 + 2*4); a swapped term changes the total.
    assert_eq!(cube_sub(2, 3, 4).face_count(), 52);
    assert_eq!(cube_sub(4, 3, 2).face_count(), 52);
    assert_eq!(cube_sub(2, 2, 5).face_count(), 2 * (4 + 10 + 10));
}

#[test]
fn cylinder_and_cone_height_divisions_add_rings() {
    assert_eq!(round(Primitive::Cylinder, 8, 3).face_count(), 8 * 3 + 2);
    assert_eq!(round(Primitive::Cone, 8, 3).face_count(), 8 * 3 + 1);
}

#[test]
fn cylinder_and_cone_accept_division_edges() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        for (axis, height) in [(3, 1), (128, 128)] {
            assert_eq!(round(primitive, axis, height).validate(primitive), Ok(()));
        }
    }
}
