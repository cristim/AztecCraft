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

/// The polySphere command page lists radius 0.5, the node page and the Create menu 1.0 (#43, #453).
/// Policy table row 53 records the difference; changing the default must update both.
#[test]
fn the_sphere_default_radius_is_one_not_the_command_page_half() {
    assert_eq!(DEFAULT_RADIUS_CM, 1.0);
    let PrimitiveOptions::Sphere { radius, .. } = PrimitiveOptions::default_for(Primitive::Sphere)
    else {
        panic!("sphere default is not a sphere");
    };
    assert_eq!(radius, 1.0);
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
            sub_caps: 0,
            round_cap: false,
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
            sub_caps: 0,
            round_cap: false,
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
fn torus_ranges_counts_and_mismatch_are_checked() {
    let torus = |radius, section_radius, sub_axis, sub_height| PrimitiveOptions::Torus {
        radius,
        section_radius,
        twist: 0.0,
        sub_axis,
        sub_height,
        axis: Axis::Y,
    };
    let check = |o: PrimitiveOptions| o.validate(Primitive::Torus);
    assert_eq!(check(torus(1.0, 0.5, 20, 20)), Ok(()));
    assert_eq!(
        check(PrimitiveOptions::default_for(Primitive::Torus)),
        Ok(())
    );
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, MAX_SIZE_CM * 2.0] {
        assert_eq!(
            check(torus(bad, 0.5, 20, 20)),
            Err(PrimitiveOptionsError::SizeOutOfRange { field: "radius" })
        );
        assert_eq!(
            check(torus(1.0, bad, 20, 20)),
            Err(PrimitiveOptionsError::SizeOutOfRange {
                field: "section_radius"
            })
        );
    }
    for (value, ok) in [(0, false), (2, false), (3, true), (128, true), (129, false)] {
        assert_eq!(
            check(torus(1.0, 0.5, value, 20)).is_ok(),
            ok,
            "axis {value}"
        );
        assert_eq!(
            check(torus(1.0, 0.5, 20, value)).is_ok(),
            ok,
            "height {value}"
        );
    }
    assert_eq!(torus(1.0, 0.5, 8, 6).face_count(), 48);
    assert_eq!(
        torus(1.0, 0.5, u32::MAX, u32::MAX).face_count(),
        u64::from(u32::MAX) * u64::from(u32::MAX)
    );
    assert_eq!(check(torus(1.0, 0.5, 128, 128)), Ok(()));
    assert_eq!(
        check(PrimitiveOptions::default_for(Primitive::Sphere)),
        Err(PrimitiveOptionsError::PrimitiveMismatch {
            expected: Primitive::Torus
        })
    );
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
        Primitive::Torus,
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
            }
            | PrimitiveOptions::Torus {
                sub_axis,
                sub_height,
                ..
            } => {
                *sub_axis = MAX_DIVISIONS;
                *sub_height = MAX_DIVISIONS;
            }
            PrimitiveOptions::Cube { .. } | PrimitiveOptions::PlatonicSolid { .. } => {
                unreachable!()
            }
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
            sub_caps,
            round_cap,
            axis,
            ..
        } => PrimitiveOptions::Cylinder {
            radius,
            height,
            sub_axis,
            sub_height,
            sub_caps,
            round_cap,
            axis,
        },
        PrimitiveOptions::Cone {
            radius,
            height,
            sub_caps,
            round_cap,
            axis,
            ..
        } => PrimitiveOptions::Cone {
            radius,
            height,
            sub_axis,
            sub_height,
            sub_caps,
            round_cap,
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

#[test]
fn twist_accepts_any_finite_angle_and_rejects_the_rest() {
    let torus = |twist| PrimitiveOptions::Torus {
        radius: 1.0,
        section_radius: 0.5,
        twist,
        sub_axis: 8,
        sub_height: 8,
        axis: Axis::Y,
    };
    for ok in [0.0, -0.0, 45.0, -720.0, 1e300, f64::MAX, f64::MIN] {
        assert_eq!(torus(ok).validate(Primitive::Torus), Ok(()), "{ok}");
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            torus(bad).validate(Primitive::Torus),
            Err(PrimitiveOptionsError::TwistNotFinite)
        );
    }
    assert_eq!(DEFAULT_TORUS_TWIST_DEG, 0.0);
    assert!(matches!(
        PrimitiveOptions::default_for(Primitive::Torus),
        PrimitiveOptions::Torus { twist, .. } if twist == 0.0
    ));
}

fn with_caps(
    primitive: Primitive,
    sub_axis: u32,
    sub_caps: u32,
    round_cap: bool,
) -> PrimitiveOptions {
    let mut o = round(primitive, sub_axis, 1);
    if let PrimitiveOptions::Cylinder {
        sub_caps: sc,
        round_cap: rc,
        ..
    }
    | PrimitiveOptions::Cone {
        sub_caps: sc,
        round_cap: rc,
        ..
    } = &mut o
    {
        *sc = sub_caps;
        *rc = round_cap;
    }
    o
}

#[test]
fn cap_defaults_are_zero_and_flat() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        assert_eq!(
            PrimitiveOptions::default_for(primitive),
            with_caps(primitive, DEFAULT_AXIS_DIVISIONS, 0, false)
        );
    }
}

#[test]
fn cap_faces_are_one_ngon_at_zero_and_axis_times_rings_otherwise() {
    // Cylinder: 20 side faces; cone: 20 side faces.
    for (sc, cyl, cone) in [(0, 22, 21), (1, 60, 40), (2, 100, 60), (128, 5140, 2580)] {
        assert_eq!(
            with_caps(Primitive::Cylinder, 20, sc, false).face_count(),
            cyl
        );
        assert_eq!(with_caps(Primitive::Cone, 20, sc, false).face_count(), cone);
    }
}

#[test]
fn cap_divisions_are_bounded_and_the_budget_counts_caps() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        assert_eq!(
            with_caps(primitive, 20, 128, false).validate(primitive),
            Ok(())
        );
        assert_eq!(
            with_caps(primitive, 20, 129, false).validate(primitive),
            Err(PrimitiveOptionsError::DivisionsOutOfRange {
                field: "subdivisions_caps",
                min: 0,
                max: MAX_DIVISIONS
            })
        );
    }
    // 128 axis x 128 caps: 2 caps of 16,384 faces plus 128 side faces (cone: one cap).
    assert_eq!(
        with_caps(Primitive::Cylinder, 128, 128, false).validate(Primitive::Cylinder),
        Err(PrimitiveOptionsError::TooManyFaces {
            faces: 32_896,
            budget: MAX_FACES
        })
    );
    assert_eq!(
        with_caps(Primitive::Cone, 128, 128, false).validate(Primitive::Cone),
        Ok(())
    );
    // 128 + 2 * 128 * sc: sc 77 is 19,840 (accepted), sc 78 is 20,096 (over the budget).
    assert!(matches!(
        with_caps(Primitive::Cylinder, 128, 78, false).validate(Primitive::Cylinder),
        Err(PrimitiveOptionsError::TooManyFaces { .. })
    ));
    assert_eq!(
        with_caps(Primitive::Cylinder, 128, 77, false).validate(Primitive::Cylinder),
        Ok(())
    );
}

#[test]
fn round_cap_at_zero_caps_is_an_accepted_no_op() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        let flat = with_caps(primitive, 20, 0, false);
        let round = with_caps(primitive, 20, 0, true);
        assert_eq!(round.validate(primitive), Ok(()));
        assert_eq!(round.face_count(), flat.face_count());
    }
}
