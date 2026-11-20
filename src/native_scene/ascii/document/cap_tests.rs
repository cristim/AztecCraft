//! Cap-subdivided primitives (#453) survive an extrude on a cap face and an ASCII save and reopen.
use super::tests::authored;
use super::*;
use crate::{
    native_scene::ascii::write::{DefaultLinks, emit_with_links},
    primitive_options::PrimitiveOptions,
    scene::Primitive,
};

fn capped(primitive: Primitive, round_cap: bool) -> NativeScene {
    let mut options = PrimitiveOptions::default_for(primitive);
    if let PrimitiveOptions::Cylinder {
        sub_caps,
        round_cap: round,
        ..
    }
    | PrimitiveOptions::Cone {
        sub_caps,
        round_cap: round,
        ..
    } = &mut options
    {
        *sub_caps = 2;
        *round = round_cap;
    }
    let mut scene = NativeScene::default();
    scene
        .create_with_options(primitive, &options, "c", false)
        .unwrap();
    scene
}

/// Corner positions of face `face`, in loop order, in centimetres.
fn corners(scene: &NativeScene, face: usize) -> Vec<[f64; 3]> {
    let mesh = scene.shapes()[0].mesh();
    mesh.faces()[face]
        .outer()
        .iter()
        .map(|d| {
            let [start, end] = mesh.edges()[d.edge].endpoints();
            mesh.effective_point_cm(if d.reversed { end } else { start })
                .unwrap()
        })
        .collect()
}

#[test]
fn a_cap_quad_extrudes_along_its_normal_by_exactly_the_distance() {
    // Default 20x1, radius 1, height 2: 20 side faces, then the cap's first ring of 20 quads
    // (extrude takes quads, not the fan triangles). Cylinder top cap faces +y, cone base -y.
    for (primitive, rim_y, facing) in [
        (Primitive::Cylinder, 1.0, 1.0),
        (Primitive::Cone, -1.0, -1.0),
    ] {
        let mut scene = capped(primitive, false);
        let id = scene.shapes()[0].id();
        let faces = scene.shapes()[0].mesh().faces().len();
        let cap_quad = 20;
        let before = corners(&scene, cap_quad);
        assert_eq!(before.len(), 4, "{primitive:?} is a quad");
        assert!(
            before.iter().all(|c| c[1] == rim_y),
            "{primitive:?} lies on the cap plane"
        );
        scene.extrude_face(id, cap_quad, 0.25).unwrap();
        let mesh = scene.shapes()[0].mesh();
        assert_eq!(
            mesh.faces().len(),
            faces + 4,
            "{primitive:?} adds four side faces"
        );
        mesh.validate().unwrap();
        let after = corners(&scene, cap_quad);
        for (old, new) in before.iter().zip(&after) {
            assert!(
                (new[1] - (rim_y + facing * 0.25)).abs() < 1e-6,
                "{primitive:?} {new:?}"
            );
            assert!((new[0] - old[0]).abs() < 1e-6 && (new[2] - old[2]).abs() < 1e-6);
        }
    }
}

#[test]
fn sub_cap_cylinders_and_cones_round_trip_through_ascii() {
    for primitive in [Primitive::Cylinder, Primitive::Cone] {
        for round_cap in [false, true] {
            let scene = capped(primitive, round_cap);
            let bytes = emit_with_links(&scene, DefaultLinks::Omit).unwrap();
            let projection = authored(&bytes);
            assert_eq!(projection.scene().shapes(), scene.shapes(), "{primitive:?}");
            assert_eq!(
                emit_with_links(projection.scene(), DefaultLinks::Omit).unwrap(),
                bytes
            );
        }
    }
}
