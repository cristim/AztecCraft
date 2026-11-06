use crate::native_scene::{test_fixtures::cube, *};
use glam::DVec3;

type Row = [[f64; 4]; 4];

fn identity() -> Row {
    let mut m = [[0.0; 4]; 4];
    for (i, row) in m.iter_mut().enumerate() {
        row[i] = 1.0;
    }
    m
}

fn mul(a: &Row, b: &Row) -> Row {
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            for k in 0..4 {
                out[i][j] += a[i][k] * b[k][j];
            }
        }
    }
    out
}

fn translation(v: [f64; 3]) -> Row {
    let mut m = identity();
    m[3][..3].copy_from_slice(&v);
    m
}

fn scaling(v: [f64; 3]) -> Row {
    let mut m = identity();
    for i in 0..3 {
        m[i][i] = v[i];
    }
    m
}

/// Row-vector rotation about `axis` by `a` (p' = p * R).
fn rot(axis: usize, a: f64) -> Row {
    let (s, c) = a.sin_cos();
    let mut m = identity();
    let (i, j) = match axis {
        0 => (1, 2),
        1 => (2, 0),
        _ => (0, 1),
    };
    m[i][i] = c;
    m[i][j] = s;
    m[j][i] = -s;
    m[j][j] = c;
    m
}

/// Row-vector Euler rotation applying `axes[0]` first.
fn euler(axes: [usize; 3], r: [f64; 3]) -> Row {
    mul(
        &mul(&rot(axes[0], r[axes[0]]), &rot(axes[1], r[axes[1]])),
        &rot(axes[2], r[axes[2]]),
    )
}

fn shearing(xy: f64, xz: f64, yz: f64) -> Row {
    let mut m = identity();
    m[1][0] = xy;
    m[2][0] = xz;
    m[2][1] = yz;
    m
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    v.map(|c| -c)
}

struct Case {
    t: [f64; 3],
    r: [f64; 3],
    s: [f64; 3],
    rp: [f64; 3],
    sp: [f64; 3],
    order: u8,
    ra: [f64; 3],
    sh: [f64; 3],
    rpt: [f64; 3],
    spt: [f64; 3],
}

const AXES: [[usize; 3]; 6] = [
    [0, 1, 2],
    [1, 2, 0],
    [2, 0, 1],
    [0, 2, 1],
    [1, 0, 2],
    [2, 1, 0],
];

fn case(order: u8) -> Case {
    Case {
        t: [1.5, -2.0, 0.75],
        r: [0.3, -0.7, 1.1],
        s: [1.2, 0.8, 1.9],
        rp: [0.4, 1.3, -0.6],
        sp: [-0.9, 0.2, 0.5],
        order,
        ra: [0.21, 0.37, -0.13],
        sh: [0.25, -0.35, 0.45],
        rpt: [0.6, -0.1, 0.8],
        spt: [-0.3, 0.9, 0.2],
    }
}

fn internal(c: &Case) -> InternalTransform {
    InternalTransform {
        translation_cm: c.t,
        rotation_radians: c.r,
        scale: c.s,
        rotate_pivot_cm: c.rp,
        scale_pivot_cm: c.sp,
        extras: Some(InternalExtras {
            rotate_order: c.order,
            rotate_axis_radians: c.ra,
            shear: c.sh,
            rotate_pivot_translate_cm: c.rpt,
            scale_pivot_translate_cm: c.spt,
        }),
        inverse_parent_scale: None,
    }
}

/// Documented row-vector chain: SP^-1 * S * SH * SP * ST * RP^-1 * RA * R * RP * RT * T.
fn reference_matrix(c: &Case) -> Row {
    let chain = [
        translation(neg(c.sp)),
        scaling(c.s),
        shearing(c.sh[0], c.sh[1], c.sh[2]),
        translation(c.sp),
        translation(c.spt),
        translation(neg(c.rp)),
        euler([0, 1, 2], c.ra),
        euler(AXES[usize::from(c.order)], c.r),
        translation(c.rp),
        translation(c.rpt),
        translation(c.t),
    ];
    chain.iter().fold(identity(), |acc, m| mul(&acc, m))
}

fn apply_row(m: &Row, p: [f64; 3]) -> [f64; 3] {
    let v = [p[0], p[1], p[2], 1.0];
    let mut out = [0.0; 3];
    for (j, slot) in out.iter_mut().enumerate() {
        *slot = (0..4).map(|k| v[k] * m[k][j]).sum();
    }
    out
}

/// Images of the unit axes under RA then R(order), as row-vector directions (no translation).
pub(crate) fn row_vector_axes(order: u8, r: [f64; 3], ra: [f64; 3]) -> [[f64; 3]; 3] {
    let m = mul(&euler([0, 1, 2], ra), &euler(AXES[usize::from(order)], r));
    [0, 1, 2].map(|i| [m[i][0], m[i][1], m[i][2]])
}

fn assert_matches(c: &Case) {
    let column = internal(c).matrix().unwrap();
    let row = reference_matrix(c);
    for p in [[1.0, 2.0, 3.0], [-0.5, 0.25, 4.0], [0.0, 0.0, 0.0]] {
        let want = apply_row(&row, p);
        let got = column.transform_point3(DVec3::from_array(p)).to_array();
        for i in 0..3 {
            assert!(
                (want[i] - got[i]).abs() < 1e-12,
                "order {} point {p:?} axis {i}: {want:?} vs {got:?}",
                c.order
            );
        }
    }
}

#[test]
fn all_six_rotate_orders_match_an_independent_row_vector_multiply() {
    for order in 0..6 {
        assert_matches(&case(order));
    }
}

#[test]
fn non_default_rotate_order_alone_matches_the_row_vector_reference() {
    for order in 1..6 {
        let mut c = case(order);
        c.ra = [0.0; 3];
        c.sh = [0.0; 3];
        c.rpt = [0.0; 3];
        c.spt = [0.0; 3];
        assert_matches(&c);
    }
}

#[test]
fn rotate_order_changes_the_result_at_a_non_symmetric_angle() {
    let matrices: Vec<_> = (0..6)
        .map(|order| {
            let mut c = case(order);
            c.ra = [0.0; 3];
            c.sh = [0.0; 3];
            c.rpt = [0.0; 3];
            c.spt = [0.0; 3];
            internal(&c).matrix().unwrap()
        })
        .collect();
    for i in 0..6 {
        for j in i + 1..6 {
            assert_ne!(matrices[i], matrices[j], "orders {i} and {j} collide");
        }
    }
}

#[test]
fn shear_sign_convention_is_provisional() {
    // PROVISIONAL: Maya row layout [1][0]=xy, [2][0]=xz, [2][1]=yz is unconfirmed
    // against a Maya reference scene.
    let c = Case {
        t: [0.0; 3],
        r: [0.0; 3],
        s: [1.0; 3],
        rp: [0.0; 3],
        sp: [0.0; 3],
        order: 0,
        ra: [0.0; 3],
        sh: [0.5, 0.0, 0.0],
        rpt: [0.0; 3],
        spt: [0.0; 3],
    };
    let m = internal(&c).matrix().unwrap();
    assert_eq!(
        m.transform_point3(DVec3::new(0.0, 2.0, 0.0)),
        DVec3::new(1.0, 2.0, 0.0)
    );
    assert_eq!(
        m.transform_point3(DVec3::new(2.0, 0.0, 0.0)),
        DVec3::new(2.0, 0.0, 0.0)
    );
    let c = Case {
        sh: [0.0, 0.5, 0.25],
        ..c
    };
    let m = internal(&c).matrix().unwrap();
    assert_eq!(
        m.transform_point3(DVec3::new(0.0, 0.0, 2.0)),
        DVec3::new(1.0, 0.5, 2.0)
    );
}

fn old_matrix(t: InternalTransform) -> DMat4 {
    let rp = DVec3::from_array(t.rotate_pivot_cm);
    let sp = DVec3::from_array(t.scale_pivot_cm);
    let r = DVec3::from_array(t.rotation_radians);
    DMat4::from_translation(DVec3::from_array(t.translation_cm))
        * DMat4::from_translation(rp)
        * DMat4::from_rotation_z(r.z)
        * DMat4::from_rotation_y(r.y)
        * DMat4::from_rotation_x(r.x)
        * DMat4::from_translation(-rp)
        * DMat4::from_translation(sp)
        * DMat4::from_scale(DVec3::from_array(t.scale))
        * DMat4::from_translation(-sp)
}

fn bits(m: DMat4) -> [u64; 16] {
    m.to_cols_array().map(f64::to_bits)
}

#[test]
fn default_extras_are_bit_identical_to_the_legacy_matrix_including_negative_zero() {
    let (mut scene, id) = cube();
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    let values = [-0.0, 1.0, 2.0, -0.0, 0.0, -0.0, 1.0, -0.0, 3.0];
    scene.nodes[index].channels = values.map(Scalar::new);
    let legacy = scene.internal_transform(id).unwrap();
    assert!(legacy.extras.is_none());
    let expected = bits(old_matrix(legacy));
    assert_eq!(bits(legacy.matrix().unwrap()), expected);
    for extras in [
        TransformExtras {
            rotate_order: Some(0),
            rotate_axis: Some([0.0; 3]),
            shear: Some([-0.0; 3]),
            rotate_pivot_translate: Some([0.0; 3]),
            scale_pivot_translate: Some([0.0; 3]),
        },
        TransformExtras::default(),
    ] {
        scene.nodes[index].extras = extras;
        let internal = scene.internal_transform(id).unwrap();
        assert!(internal.extras.is_none());
        assert_eq!(bits(internal.matrix().unwrap()), expected);
    }
}

#[test]
fn rejects_non_finite_values_and_out_of_range_rotate_order() {
    let (mut scene, id) = cube();
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for slot in 0..4 {
            let mut e = TransformExtras::default();
            let v = Some([0.0, bad, 0.0]);
            match slot {
                0 => e.rotate_axis = v,
                1 => e.shear = v,
                2 => e.rotate_pivot_translate = v,
                _ => e.scale_pivot_translate = v,
            }
            scene.nodes[index].extras = e;
            assert_eq!(scene.validate(), Err(NativeError::NonFinite));
            assert_eq!(scene.internal_transform(id), Err(NativeError::NonFinite));
        }
    }
    scene.nodes[index].extras = TransformExtras {
        rotate_order: Some(6),
        ..TransformExtras::default()
    };
    assert_eq!(scene.validate(), Err(NativeError::InvalidTransformExtras));
    assert_eq!(
        scene.internal_transform(id),
        Err(NativeError::InvalidTransformExtras)
    );
}

#[test]
fn extras_convert_per_field_units() {
    let (mut scene, id) = cube();
    scene.units.linear = LinearUnit::Meter;
    scene.units.angular = AngularUnit::Radian;
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    scene.nodes[index].extras = TransformExtras {
        rotate_order: Some(2),
        rotate_axis: Some([1.0, 0.0, 0.0]),
        shear: Some([0.5, 0.0, 0.0]),
        rotate_pivot_translate: Some([1.0, 0.0, 0.0]),
        scale_pivot_translate: Some([0.0, 2.0, 0.0]),
    };
    let e = scene.internal_transform(id).unwrap().extras.unwrap();
    assert_eq!(e.rotate_order, 2);
    assert_eq!(e.rotate_axis_radians, [1.0, 0.0, 0.0]);
    assert_eq!(e.shear, [0.5, 0.0, 0.0]);
    assert_eq!(e.rotate_pivot_translate_cm, [100.0, 0.0, 0.0]);
    assert_eq!(e.scale_pivot_translate_cm, [0.0, 200.0, 0.0]);
    scene.units.angular = AngularUnit::Degree;
    let e = scene.internal_transform(id).unwrap().extras.unwrap();
    assert_eq!(e.rotate_axis_radians, [1f64.to_radians(), 0.0, 0.0]);
}

#[test]
fn legacy_supported_view_refuses_non_default_extras_but_edit_checks_do_not() {
    let (mut scene, id) = cube();
    assert!(scene.supported_view().is_ok());
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    scene.nodes[index].extras = TransformExtras {
        shear: Some([0.1, 0.0, 0.0]),
        ..TransformExtras::default()
    };
    assert!(matches!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView(_))
    ));
    assert!(scene.check_effects(scene.nodes[index].transform_id).is_ok());
    scene.nodes[index].extras = TransformExtras {
        rotate_order: Some(0),
        ..TransformExtras::default()
    };
    assert!(scene.supported_view().is_ok());
}

#[test]
fn rotate_order_alone_survives_the_scene_path_and_matches_the_reference() {
    for order in 1..=5u8 {
        let (mut scene, id) = cube();
        let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
        scene.units.angular = AngularUnit::Radian;
        scene.nodes[index].channels =
            [0.0, 0.0, 0.0, 0.3, -0.7, 1.1, 1.0, 1.0, 1.0].map(Scalar::new);
        scene.nodes[index].extras = TransformExtras {
            rotate_order: Some(order),
            ..TransformExtras::default()
        };
        let internal = scene.internal_transform(id).unwrap();
        assert!(
            internal.extras.is_some(),
            "order {order} must not be default"
        );
        let reference = Case {
            t: [0.0; 3],
            r: [0.3, -0.7, 1.1],
            s: [1.0; 3],
            rp: [0.0; 3],
            sp: [0.0; 3],
            order,
            ra: [0.0; 3],
            sh: [0.0; 3],
            rpt: [0.0; 3],
            spt: [0.0; 3],
        };
        let want = reference_matrix(&reference);
        let got = internal.matrix().unwrap();
        for p in [[1.0, 2.0, 3.0], [-0.5, 0.25, 4.0]] {
            let w = apply_row(&want, p);
            let g = got.transform_point3(DVec3::from_array(p)).to_array();
            for i in 0..3 {
                assert!((w[i] - g[i]).abs() < 1e-12, "order {order}: {w:?} vs {g:?}");
            }
        }
    }
}

#[test]
fn shear_is_unitless_in_a_degree_scene() {
    let (mut scene, id) = cube();
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    assert_eq!(scene.units.angular, AngularUnit::Degree);
    scene.nodes[index].extras = TransformExtras {
        shear: Some([0.5, 0.25, -0.75]),
        ..TransformExtras::default()
    };
    let e = scene.internal_transform(id).unwrap().extras.unwrap();
    assert_eq!(e.shear, [0.5, 0.25, -0.75]);
}

#[test]
fn out_of_range_rotate_order_has_no_matrix() {
    let extras = InternalExtras {
        rotate_order: 6,
        rotate_axis_radians: [0.0; 3],
        shear: [0.0; 3],
        rotate_pivot_translate_cm: [0.0; 3],
        scale_pivot_translate_cm: [0.0; 3],
    };
    let t = InternalTransform {
        translation_cm: [0.0; 3],
        rotation_radians: [0.1; 3],
        scale: [1.0; 3],
        rotate_pivot_cm: [0.0; 3],
        scale_pivot_cm: [0.0; 3],
        extras: Some(extras),
        inverse_parent_scale: None,
    };
    assert!(t.matrix().is_none());
}

#[test]
fn transform_attributes_report_names_degrees_and_declared_linear_units() {
    let (mut scene, id) = cube();
    let plain = scene.transform_attributes(id).unwrap();
    assert_eq!(plain, TransformAttributes::default());
    assert!(plain.non_default.is_empty());
    scene.units.angular = AngularUnit::Radian;
    scene.units.linear = LinearUnit::Meter;
    let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
    scene.nodes[index].extras = TransformExtras {
        rotate_order: Some(1),
        rotate_axis: Some([std::f64::consts::FRAC_PI_2, 0.0, 0.0]),
        shear: Some([0.5, 0.0, 0.25]),
        rotate_pivot_translate: Some([1.0, 2.0, 3.0]),
        scale_pivot_translate: Some([0.0; 3]),
    };
    let got = scene.transform_attributes(id).unwrap();
    assert_eq!(got.rotate_order, "yzx");
    assert!((got.rotate_axis_degrees[0] - 90.0).abs() < 1e-12);
    // Pivot translations stay in the declared unit (meters), like authoring_transform.
    assert_eq!(got.rotate_pivot_translate, [1.0, 2.0, 3.0]);
    assert_eq!(
        got.non_default,
        [
            "rotate_order",
            "rotate_axis",
            "shear",
            "rotate_pivot_translate"
        ]
    );
    scene.nodes[index].extras.rotate_order = Some(6);
    assert!(scene.transform_attributes(id).is_err());
}

#[test]
fn degree_scenes_report_the_declared_extras_bit_for_bit() {
    for axis in [[10.0, 20.0, -30.0], [12.345, -0.1, 7.0]] {
        let (mut scene, id) = cube();
        let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
        scene.nodes[index].extras = TransformExtras {
            rotate_axis: Some(axis),
            rotate_pivot_translate: Some([12.345, -0.1, 7.0]),
            ..TransformExtras::default()
        };
        let got = scene.transform_attributes(id).unwrap();
        assert_eq!(
            got.rotate_axis_degrees.map(f64::to_bits),
            axis.map(f64::to_bits)
        );
        assert_eq!(
            got.rotate_pivot_translate.map(f64::to_bits),
            [12.345, -0.1, 7.0].map(f64::to_bits)
        );
    }
}

#[test]
fn every_rotate_order_maps_to_its_maya_name() {
    let names = ["xyz", "yzx", "zxy", "xzy", "yxz", "zyx"];
    for (order, name) in names.into_iter().enumerate() {
        let (mut scene, id) = cube();
        let index = scene.nodes.iter().position(|n| n.id == id).unwrap();
        scene.nodes[index].extras = TransformExtras {
            rotate_order: Some(order as u8),
            ..TransformExtras::default()
        };
        let got = scene.transform_attributes(id).unwrap();
        assert_eq!(got.rotate_order, name, "order {order}");
        assert_eq!(got.non_default.contains(&"rotate_order"), order != 0);
    }
}
