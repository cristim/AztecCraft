use super::grid_tests::{assert_closed_manifold, euler, extent, face_area_vector, loops, points};
use super::*;
use crate::primitive_options::Axis;

fn torus(
    radius: f64,
    section_radius: f64,
    sub_axis: u32,
    sub_height: u32,
    axis: Axis,
) -> NativeMesh {
    NativeMesh::with_options(&PrimitiveOptions::Torus {
        radius,
        section_radius,
        sub_axis,
        sub_height,
        axis,
    })
    .unwrap()
}

#[test]
fn torus_counts_follow_the_closed_forms_and_euler_is_zero() {
    for (sa, sh) in [(3, 3), (5, 4), (13, 9), (20, 20), (3, 128)] {
        let mesh = torus(2.0, 0.5, sa, sh, Axis::Y);
        let (sa, sh) = (sa as usize, sh as usize);
        assert_eq!(mesh.point_count(), sa * sh);
        assert_eq!(mesh.edges.len(), 2 * sa * sh);
        assert_eq!(mesh.faces.len(), sa * sh);
        assert_eq!(euler(&mesh), 0);
        assert!(loops(&mesh).iter().all(|face| face.len() == 4));
        assert_closed_manifold(&mesh);
        mesh.validate().unwrap();
    }
}

#[test]
fn default_torus_is_a_radius_one_tube_of_half_a_centimetre_with_400_quads() {
    let mesh = NativeMesh::primitive(Primitive::Torus).unwrap();
    assert_eq!(
        (mesh.point_count(), mesh.edges.len(), mesh.faces.len()),
        (400, 800, 400)
    );
    assert_eq!(extent(&mesh, 0), (-1.5, 1.5));
    assert_eq!(extent(&mesh, 1), (-0.5, 0.5));
    assert_eq!(extent(&mesh, 2), (-1.5, 1.5));
}

#[test]
fn torus_hole_axis_and_outward_winding_follow_the_axis() {
    for (axis, up) in [(Axis::Y, 1), (Axis::X, 0), (Axis::Z, 2)] {
        let mesh = torus(2.0, 0.5, 9, 8, axis);
        let p = points(&mesh);
        assert_eq!(extent(&mesh, up), (-0.5, 0.5), "{axis:?} thickness");
        for face in loops(&mesh) {
            let n = face_area_vector(&mesh, &face);
            let centre: [f64; 3] = std::array::from_fn(|c| {
                face.iter().map(|i| f64::from(p[*i][c])).sum::<f64>() / face.len() as f64
            });
            // Away from the tube's core circle: the ring-plane component of the centre
            // is scaled to the ring radius.
            let planar: Vec<usize> = (0..3).filter(|c| *c != up).collect();
            let len = (centre[planar[0]].powi(2) + centre[planar[1]].powi(2)).sqrt();
            let mut core = centre;
            for c in &planar {
                core[*c] -= centre[*c] / len * 2.0;
            }
            assert!(
                n[0] * core[0] + n[1] * core[1] + n[2] * core[2] > 0.0,
                "{axis:?} face {face:?} points into the tube"
            );
        }
    }
}

#[test]
fn a_section_radius_larger_than_the_ring_radius_is_accepted() {
    // PROVISIONAL: the Maya pages state no relation between the two radii.
    torus(0.5, 2.0, 4, 4, Axis::Y).validate().unwrap();
}
