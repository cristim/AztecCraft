use super::*;
use crate::native_scene::{
    ALL_CHANNELS, AngularUnit, LinearUnit, NativeTransform, Scalar, Units, ascii, binary,
    test_fixtures::{cube, restrict_edits, set_extras},
};
use crate::scene::{Primitive, SceneError};
use glam::{DMat4, DVec3};
use std::sync::Arc;

mod ascii_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_ascii.rs"
    ));
}
#[allow(dead_code)]
mod wire {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

#[derive(Clone)]
struct Case {
    name: &'static str,
    linear: LinearUnit,
    angular: AngularUnit,
    t: [f64; 3],
    r_degrees: [f64; 3],
    s: [f64; 3],
    rp: [f64; 3],
    sp: [f64; 3],
    order: u8,
    ra_degrees: [f64; 3],
    sh: [f64; 3],
    rpt: [f64; 3],
    spt: [f64; 3],
    /// The pivot targets, in declared units.
    d: [f64; 3],
}

fn base(name: &'static str) -> Case {
    Case {
        name,
        linear: LinearUnit::Centimeter,
        angular: AngularUnit::Degree,
        t: [1.5, -2.0, 0.75],
        r_degrees: [20.0, -35.0, 50.0],
        s: [1.2, 0.8, 1.9],
        rp: [0.4, 1.3, -0.6],
        sp: [-0.9, 0.2, 0.5],
        order: 0,
        ra_degrees: [0.0; 3],
        sh: [0.0; 3],
        rpt: [0.0; 3],
        spt: [0.0; 3],
        d: [0.7, -1.1, 2.3],
    }
}

fn table() -> Vec<Case> {
    let mut cases = vec![
        Case {
            r_degrees: [0.0; 3],
            s: [1.0; 3],
            rp: [0.0; 3],
            sp: [0.0; 3],
            ..base("identity")
        },
        base("rotated"),
        Case {
            s: [3.0, 0.25, -2.0],
            ..base("non-uniform scale")
        },
        Case {
            sh: [0.25, -0.35, 0.45],
            ..base("shear")
        },
        Case {
            ra_degrees: [12.0, -7.0, 25.0],
            ..base("rotate axis")
        },
        Case {
            rpt: [0.6, -0.1, 0.8],
            spt: [-0.3, 0.9, 0.2],
            ..base("rpt and spt")
        },
        Case {
            angular: AngularUnit::Radian,
            ra_degrees: [12.0, -7.0, 25.0],
            ..base("radian scene")
        },
        Case {
            linear: LinearUnit::Meter,
            rpt: [0.6, -0.1, 0.8],
            spt: [-0.3, 0.9, 0.2],
            sh: [0.25, -0.35, 0.45],
            ..base("meter scene")
        },
        Case {
            t: [1e6, -2e6, 3e6],
            rp: [1e6, 2e6, -1e6],
            sp: [-1e6, 1e6, 5e5],
            d: [7e5, -1.1e6, 2.3e6],
            ..base("1e6")
        },
        Case {
            rp: [1e-300, -1e-300, 0.0],
            sp: [2e-300, 0.0, -1e-300],
            d: [3e-300, -4e-300, 1e-300],
            ..base("1e-300")
        },
        Case {
            t: [-0.0, 1.0, -0.0],
            rp: [-0.0, 0.5, -0.0],
            sp: [-0.0; 3],
            ..base("negative zero")
        },
    ];
    for order in 0..6 {
        cases.push(Case {
            order,
            name: ["ro xyz", "ro yzx", "ro zxy", "ro xzy", "ro yxz", "ro zyx"][usize::from(order)],
            ra_degrees: [12.0, -7.0, 25.0],
            sh: [0.25, -0.35, 0.45],
            rpt: [0.6, -0.1, 0.8],
            spt: [-0.3, 0.9, 0.2],
            ..base("")
        });
    }
    cases
}

fn build(c: &Case) -> (NativeScene, NodeId) {
    let (mut scene, id) = cube();
    scene.units = Units {
        linear: c.linear.clone(),
        angular: c.angular.clone(),
        time: "film".into(),
    };
    let angle = |degrees: [f64; 3]| {
        let factor = scene.units.angular_degrees_factor().unwrap();
        degrees.map(|d| d / factor)
    };
    let channels = NativeTransform {
        translation: c.t,
        rotation: angle(c.r_degrees),
        scale: c.s,
    };
    let extras = TransformExtras {
        rotate_order: Some(c.order),
        rotate_axis: Some(angle(c.ra_degrees)),
        shear: Some(c.sh),
        rotate_pivot_translate: Some(c.rpt),
        scale_pivot_translate: Some(c.spt),
    };
    let node = &mut scene.nodes[0];
    node.channels = channels.values().map(Scalar::new);
    node.rotate_pivot = c.rp.map(Scalar::new);
    node.scale_pivot = c.sp.map(Scalar::new);
    set_extras(&mut scene, id, extras);
    scene.validate().unwrap();
    (scene, id)
}

fn matrix(scene: &NativeScene, id: NodeId) -> DMat4 {
    scene.internal_transform(id).unwrap().matrix().unwrap()
}

fn assert_same_matrix(label: &str, before: DMat4, after: DMat4) {
    let (a, b) = (before.to_cols_array(), after.to_cols_array());
    let scale = 1.0 + a.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    for (x, y) in a.iter().zip(b) {
        assert!(
            (x - y).abs() <= 1e-12 * scale,
            "{label}: {before:?} vs {after:?}"
        );
    }
}

fn bits(v: [f64; 3]) -> [u64; 3] {
    v.map(f64::to_bits)
}

#[test]
fn preserving_edits_keep_the_matrix_over_the_case_table() {
    for c in table() {
        let target = c.d;
        let rotate = Some(target);
        let scale = Some(target.map(|v| -v * 0.5));
        for (label, rotate, scale) in [
            ("rp", rotate, None),
            ("sp", None, scale),
            ("both", rotate, scale),
        ] {
            let (mut scene, id) = build(&c);
            let before = matrix(&scene, id);
            let outcome = scene.set_pivots(id, rotate, scale, true).unwrap();
            assert_eq!(outcome, PivotOutcome::Changed, "{} {label}", c.name);
            assert_eq!(
                scene.node(id).unwrap().rotate_pivot(),
                rotate.unwrap_or(c.rp)
            );
            assert_eq!(scene.node(id).unwrap().scale_pivot(), scale.unwrap_or(c.sp));
            assert_same_matrix(&format!("{} {label}", c.name), before, matrix(&scene, id));
        }
    }
}

#[test]
fn rotate_pivot_compensation_matches_a_hand_derived_translate() {
    // R = Rz(90 degrees), rp from 0 to (1, 0, 0): rt' = -(d - R d) = (-1, 1, 0).
    let c = Case {
        r_degrees: [0.0, 0.0, 90.0],
        s: [2.0, 1.0, 1.0],
        rp: [0.0; 3],
        sp: [0.0; 3],
        rpt: [0.0; 3],
        spt: [0.0; 3],
        ..base("hand")
    };
    let (mut scene, id) = build(&c);
    // sp from 0 to (1, 0, 0) with K = diag(2, 1, 1): st' = -(d - K d) = (1, 0, 0).
    scene
        .set_pivots(id, Some([1.0, 0.0, 0.0]), Some([1.0, 0.0, 0.0]), true)
        .unwrap();
    let extras = scene.node(id).unwrap().extras();
    let (rpt, spt) = (
        extras.rotate_pivot_translate.unwrap(),
        extras.scale_pivot_translate.unwrap(),
    );
    for (got, want) in rpt.into_iter().zip([-1.0, 1.0, 0.0]) {
        assert!((got - want).abs() < 1e-12, "{rpt:?}");
    }
    assert_eq!(bits(spt), bits([1.0, 0.0, 0.0]));
    // The same edit in a meter scene stores meters, not centimeters.
    let (mut meters, id) = build(&Case {
        linear: LinearUnit::Meter,
        ..c
    });
    meters
        .set_pivots(id, Some([1.0, 0.0, 0.0]), Some([1.0, 0.0, 0.0]), true)
        .unwrap();
    let rpt = meters
        .node(id)
        .unwrap()
        .extras()
        .rotate_pivot_translate
        .unwrap();
    assert!(
        (rpt[0] + 1.0).abs() < 1e-12 && (rpt[1] - 1.0).abs() < 1e-12,
        "{rpt:?}"
    );
}

#[test]
fn an_exactly_zero_compensation_leaves_the_extras_untouched() {
    // Rs = I and K = I: nothing to compensate, so `None` stays `None`.
    let (mut scene, id) = cube();
    scene.nodes[0].scale_pivot = [0.5, 0.0, 0.0].map(Scalar::new);
    let before = scene.nodes[0].extras.clone();
    assert_eq!(before, TransformExtras::default());
    scene
        .set_pivots(id, Some([1.0, 2.0, 3.0]), Some([-4.0, 5.0, 6.0]), true)
        .unwrap();
    assert_eq!(scene.nodes[0].extras, before);
    // An authored `Some(-0.0)` triple survives bit for bit as well.
    let mut kept = scene.clone();
    let negative = [-0.0, 0.0, -0.0];
    kept.nodes[0].extras.rotate_pivot_translate = Some(negative);
    kept.set_pivots(id, Some([7.0, 8.0, 9.0]), None, true)
        .unwrap();
    assert_eq!(
        bits(kept.nodes[0].extras.rotate_pivot_translate.unwrap()),
        bits(negative)
    );
}

#[test]
fn only_components_with_a_nonzero_compensation_change() {
    // Rotation about Z, rp.z set to -0.0 and rp.y moved: v_z is exactly zero, v_x and v_y are not.
    let (mut scene, id) = cube();
    scene.nodes[0].channels[5] = Scalar::new(40.0);
    scene.nodes[0].extras.rotate_pivot_translate = Some([0.0, 2.0, -0.0]);
    scene
        .set_pivots(id, Some([0.0, 1.0, -0.0]), None, true)
        .unwrap();
    let rpt = scene.nodes[0].extras.rotate_pivot_translate.unwrap();
    assert_eq!(rpt[2].to_bits(), (-0.0f64).to_bits());
    assert_ne!(rpt[0], 0.0);
    assert_ne!(rpt[1], 2.0);
}

#[test]
fn an_all_zero_result_is_stored_as_none() {
    // K = diag(2, 1, 1), sp.x from 0 to 3: v_x = -3, so spt.x = -3 + 3 = 0 and the triple is zero.
    let (mut scene, id) = cube();
    scene.nodes[0].channels[6] = Scalar::new(2.0);
    scene.nodes[0].extras.scale_pivot_translate = Some([-3.0, 0.0, 0.0]);
    scene
        .set_pivots(id, None, Some([3.0, 0.0, 0.0]), true)
        .unwrap();
    assert_eq!(scene.nodes[0].extras, TransformExtras::default());
}

#[test]
fn an_edit_that_changes_nothing_is_unchanged() {
    let (mut scene, id) = build(&base("same"));
    let before = scene.clone();
    let rp = scene.node(id).unwrap().rotate_pivot();
    assert_eq!(
        scene.set_pivots(id, None, None, true),
        Ok(PivotOutcome::Unchanged)
    );
    assert_eq!(
        scene.set_pivots(id, Some(rp), None, true),
        Ok(PivotOutcome::Unchanged)
    );
    assert_eq!(scene, before);
}

#[test]
fn refusals_mutate_nothing() {
    let (mut scene, id) = build(&base("refusals"));
    let before = scene.clone();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            scene.set_pivots(id, Some([0.0, bad, 0.0]), None, true),
            Err(NativeError::NonFinite)
        );
        assert_eq!(
            scene.set_pivots(id, None, Some([bad, 0.0, 0.0]), true),
            Err(NativeError::NonFinite)
        );
    }
    for huge in [[1.7e308; 3], [-1.7e308, 1.7e308, 1.0]] {
        assert_eq!(
            scene.set_pivots(id, Some(huge), None, true),
            Err(NativeError::NonFinite)
        );
        assert_eq!(
            scene.set_pivots(id, None, Some(huge), true),
            Err(NativeError::NonFinite)
        );
    }
    assert_eq!(
        scene.set_pivots(NodeId::new(999), Some([1.0; 3]), None, true),
        Err(SceneError::MissingNode(NodeId::new(999)).into())
    );
    restrict_edits(&mut scene, id);
    let restricted = scene.clone();
    assert_eq!(
        scene.set_pivots(id, Some([1.0; 3]), None, true),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(scene, restricted);
    assert_ne!(restricted, before);
}

#[test]
fn the_new_error_has_actionable_text() {
    assert_eq!(
        NativeError::PivotNeedsTranslateCorrection.to_string(),
        "Moving this pivot without moving the object needs a pivot translate, which an opened Maya binary scene cannot store yet"
    );
}

/// One transform with the given leaves and a triangle shape under it.
fn binary_scene(leaves: &[(&[u8], [f64; 3])]) -> NativeScene {
    let mut transform = wire::creation(b"object", None);
    for (key, values) in leaves {
        transform.extend(wire::triple(key, *values));
    }
    let bytes = wire::scene(
        &[
            wire::group(b"XFRM", &transform, 4),
            wire::group(
                b"DMSH",
                &[
                    wire::creation(b"shape", Some(b"object")),
                    wire::chunk(b"MESH", &wire::mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    );
    binary::decode(bytes).unwrap()
}

fn opened(rotation: [f64; 3], scale: [f64; 3]) -> (NativeScene, NodeId) {
    let scene = binary_scene(&[
        (b"t", [1.0, 2.0, 3.0]),
        (b"r", rotation),
        (b"s", scale),
        (b"rp", [0.5, 0.25, -1.0]),
        (b"sp", [-1.0, 2.0, 0.5]),
    ]);
    let id = scene.nodes()[0].id();
    (scene, id)
}

#[test]
fn an_opened_binary_takes_only_edits_that_need_no_compensation() {
    // Rotation about Z only and scale 1 on Y: moving rp along Z and sp along Y is exact.
    let (mut scene, id) = opened([0.0, 0.0, 0.7], [2.0, 1.0, 1.0]);
    let before = matrix(&scene, id);
    assert_eq!(
        scene.set_pivots(id, Some([0.5, 0.25, 4.0]), Some([-1.0, 5.0, 0.5]), true),
        Ok(PivotOutcome::Changed)
    );
    assert_eq!(scene.node(id).unwrap().rotate_pivot()[2], 4.0);
    assert_eq!(scene.node(id).unwrap().scale_pivot()[1], 5.0);
    assert_same_matrix("opened exact", before, matrix(&scene, id));
    assert_eq!(scene.nodes[0].extras, TransformExtras::default());
}

#[test]
fn an_opened_binary_refuses_a_compensation_it_cannot_store() {
    let (mut scene, id) = opened([0.0, 0.0, 0.7], [2.0, 1.0, 1.0]);
    let before = scene.clone();
    for (rotate, scale) in [
        (Some([1.0, 0.25, -1.0]), None),
        (None, Some([2.0, 2.0, 0.5])),
        (Some([0.5, 0.25, 4.0]), Some([3.0, 2.0, 0.5])),
    ] {
        assert_eq!(
            scene.set_pivots(id, rotate, scale, true),
            Err(NativeError::PivotNeedsTranslateCorrection)
        );
        assert_eq!(scene, before);
    }
}

#[test]
fn an_absent_pivot_leaf_cannot_be_edited_in_an_opened_binary() {
    let scene = binary_scene(&[(b"t", [1.0, 2.0, 3.0])]);
    let id = scene.nodes()[0].id();
    let mut edited = scene.clone();
    assert_eq!(
        edited.set_pivots(id, Some([1.0, 0.0, 0.0]), None, true),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(edited, scene);
    // Writing the value the leaf already implies is not an edit.
    assert_eq!(
        edited.set_pivots(id, Some([0.0; 3]), Some([0.0; 3]), true),
        Ok(PivotOutcome::Unchanged)
    );
    edited.source.as_mut().unwrap().has_removals = true;
    let removed = edited.clone();
    assert_eq!(
        edited.set_pivots(id, None, Some([0.0, 2.0, 0.0]), true),
        Err(NativeError::InsertionAfterDelete)
    );
    assert_eq!(edited, removed);
}

#[test]
fn an_opened_ascii_scene_is_read_only() {
    let mut scene = ascii::decode(Arc::from(ascii_fixture::specimen())).unwrap();
    let id = scene.nodes()[0].id();
    let before = scene.clone();
    assert_eq!(
        scene.set_pivots(id, Some([1.0; 3]), None, true),
        Err(NativeError::ReadOnlyFormat)
    );
    assert_eq!(scene, before);
}

#[test]
fn a_raw_edit_moves_only_the_pivot_words_and_shifts_the_translation_column() {
    // Rz(90 degrees), rp from 0 to (1, 0, 0): the matrix translation moves by (I - R) d = (1, -1, 0).
    let c = Case {
        r_degrees: [0.0, 0.0, 90.0],
        rp: [0.0; 3],
        sp: [0.0; 3],
        ..base("hand")
    };
    let (mut scene, id) = build(&c);
    let before = matrix(&scene, id);
    scene
        .set_pivots(id, Some([1.0, 0.0, 0.0]), None, false)
        .unwrap();
    let after = matrix(&scene, id);
    let shift = after.w_axis - before.w_axis;
    for (got, want) in shift.to_array().into_iter().zip([1.0, -1.0, 0.0, 0.0]) {
        assert!((got - want).abs() < 1e-12, "{shift:?}");
    }
    assert_eq!(after.x_axis, before.x_axis);
    assert_eq!(scene.nodes[0].extras, build(&c).0.nodes[0].extras);
}

#[test]
fn a_raw_edit_follows_the_documented_translation_shift_over_the_case_table() {
    for c in table() {
        let d = c.d;
        let (mut scene, id) = build(&c);
        let internal = scene.internal_transform(id).unwrap();
        let before = matrix(&scene, id);
        let extras_before = scene.nodes[0].extras.clone();
        scene
            .set_pivots(id, Some(d), Some(d.map(|v| -v)), false)
            .unwrap();
        let after = matrix(&scene, id);
        let factor = scene.units.linear_factor().unwrap();
        let drp = DVec3::from_array(d) * factor - DVec3::from_array(internal.rotate_pivot_cm);
        let dsp =
            DVec3::from_array(d.map(|v| -v)) * factor - DVec3::from_array(internal.scale_pivot_cm);
        let rs = internal.rotation_frame().unwrap();
        let k = internal.scale_stage();
        let shift = (drp - rs * drp) + rs * (dsp - k * dsp);
        let mut expected = before;
        expected.w_axis += shift.extend(0.0);
        assert_same_matrix(c.name, expected, after);
        assert_eq!(scene.nodes[0].extras, extras_before, "{}", c.name);
    }
}

#[test]
fn a_raw_edit_on_an_opened_binary_patches_an_existing_leaf_only() {
    let (mut scene, id) = opened([0.0, 0.0, 0.7], [2.0, 1.0, 1.0]);
    scene
        .set_pivots(id, Some([9.0, 8.0, 7.0]), Some([6.0, 5.0, 4.0]), false)
        .unwrap();
    assert_eq!(scene.node(id).unwrap().rotate_pivot(), [9.0, 8.0, 7.0]);
    assert!(scene.nodes[0].rotate_pivot.iter().all(|v| v.replaced));
    assert_eq!(scene.nodes[0].extras, TransformExtras::default());
    let bare = binary_scene(&[(b"t", [1.0, 2.0, 3.0])]);
    let id = bare.nodes()[0].id();
    let mut edited = bare.clone();
    assert_eq!(
        edited.set_pivots(id, Some([1.0, 0.0, 0.0]), None, false),
        Err(NativeError::UnsupportedEdit)
    );
    assert_eq!(edited, bare);
}

#[test]
fn a_keyed_node_refuses_every_pivot_edit() {
    let (mut scene, keyed) = build(&base("keyed"));
    let other = scene.create(Primitive::Cube, "other").unwrap();
    scene.set_keys(&[keyed], 1, ALL_CHANNELS, 1).unwrap();
    let before = scene.clone();
    for preserve in [true, false] {
        for (rotate, scale) in [(Some([1.0; 3]), None), (None, None)] {
            assert_eq!(
                scene.set_pivots(keyed, rotate, scale, preserve),
                Err(NativeError::PivotOnKeyedNode)
            );
        }
    }
    assert_eq!(scene, before);
    assert_eq!(
        scene.set_pivots(other, Some([1.0; 3]), None, true),
        Ok(PivotOutcome::Changed)
    );
    assert_eq!(
        NativeError::PivotOnKeyedNode.to_string(),
        "Pivots cannot be edited on an animated (keyed) object"
    );
}
