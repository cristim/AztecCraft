use super::*;
use crate::native_scene::authored::{AuthoredIneligible, check_content_kind};
use crate::primitive_options::{Axis, PrimitiveOptions};
use crate::scene::Primitive;

fn scene_with(primitive: Primitive) -> (NativeScene, NativeId) {
    let mut scene = NativeScene::default();
    scene.create(primitive, "object").unwrap();
    let shape = scene.shapes()[0].id();
    (scene, shape)
}

#[test]
fn planar_fills_the_unit_square_with_one_uv_per_corner() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let set = scene.uv_set(shape).unwrap();
    assert_eq!(set.faces().len(), 6);
    assert_eq!(set.corner_count(), 24);
    let all: Vec<f64> = set.faces().iter().flatten().flatten().copied().collect();
    assert!(all.iter().all(|value| (0.0..=1.0).contains(value)));
    assert!(all.contains(&0.0) && all.contains(&1.0));
}

#[test]
fn unused_points_do_not_change_face_uv_bounds() {
    let (scene, shape) = scene_with(Primitive::Cube);
    let mesh = scene.shape(shape).unwrap().mesh();
    let mut points: Vec<[f32; 3]> = (0..mesh.point_count())
        .map(|index| mesh.effective_point_cm(index).unwrap().map(|v| v as f32))
        .collect();
    points.extend([[-100.0, -200.0, -300.0], [400.0, 500.0, 600.0]]);
    let with_unused = NativeMesh::from_binary_parts(
        points,
        mesh.edges()
            .iter()
            .cloned()
            .map(|mut edge| {
                edge.smooth = None;
                edge
            })
            .collect(),
        mesh.faces().to_vec(),
        Vec::new(),
    )
    .unwrap();
    for (axis, _, _) in AXES {
        let expected = planar(mesh, axis).unwrap();
        let actual = planar(&with_unused, axis).unwrap();
        assert_eq!(actual, expected, "{axis:?}");
        for component in 0..2 {
            let values: Vec<f64> = actual
                .faces()
                .iter()
                .flatten()
                .map(|uv| uv[component])
                .collect();
            assert!(values.iter().all(|value| (0.0..=1.0).contains(value)));
            assert!(
                values.contains(&0.0) && values.contains(&1.0),
                "{axis:?} component {component}"
            );
        }
    }
}

const AXES: [(UvProjectAxis, usize, Axis); 3] = [
    (UvProjectAxis::X, 0, Axis::X),
    (UvProjectAxis::Y, 1, Axis::Y),
    (UvProjectAxis::Z, 2, Axis::Z),
];

/// Each face's corner positions with its projected UVs, in face order.
fn face_corners(scene: &NativeScene, shape: NativeId) -> Vec<Vec<([f64; 3], [f64; 2])>> {
    let mesh = scene.shape(shape).unwrap().mesh();
    let set = scene.uv_set(shape).unwrap();
    mesh.faces()
        .iter()
        .zip(set.faces())
        .map(|(face, uvs)| {
            face.outer()
                .iter()
                .zip(uvs)
                .map(|(directed, uv)| {
                    let start = mesh.directed_start(*directed).unwrap();
                    (mesh.effective_point_cm(start).unwrap(), *uv)
                })
                .collect()
        })
        .collect()
}

/// Newell normal of the face's 3D corners, unnormalised.
fn face_normal(corners: &[([f64; 3], [f64; 2])]) -> [f64; 3] {
    let mut n = [0.0; 3];
    for (i, (a, _)) in corners.iter().enumerate() {
        let (b, _) = corners[(i + 1) % corners.len()];
        n[0] += (a[1] - b[1]) * (a[2] + b[2]);
        n[1] += (a[2] - b[2]) * (a[0] + b[0]);
        n[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    n
}

/// Shoelace area of the UV polygon: positive when the corners run counter-clockwise.
fn uv_signed_area(corners: &[([f64; 3], [f64; 2])]) -> f64 {
    let mut twice = 0.0;
    for (i, (_, a)) in corners.iter().enumerate() {
        let (_, b) = corners[(i + 1) % corners.len()];
        twice += a[0] * b[1] - b[0] * a[1];
    }
    twice / 2.0
}

fn box_options(axis: Axis) -> PrimitiveOptions {
    PrimitiveOptions::Cube {
        width: 2.0,
        height: 3.0,
        depth: 5.0,
        sub_width: 1,
        sub_height: 1,
        sub_depth: 1,
        axis,
    }
}

#[test]
fn faces_looking_along_the_axis_get_positive_uv_area() {
    for (project, axis, _) in AXES {
        let (mut scene, shape) = scene_with(Primitive::Cube);
        scene.project_uv_planar(shape, project).unwrap();
        let (mut toward, mut away) = (0, 0);
        for face in face_corners(&scene, shape) {
            let n = face_normal(&face);
            let area = uv_signed_area(&face);
            if n[axis].abs() > 0.5 {
                // Precondition: the normal points outward, so a bad fixture cannot pass vacuously.
                let centre = face.iter().map(|(p, _)| p[axis]).sum::<f64>() / face.len() as f64;
                assert!(n[axis] * centre > 0.0, "{project:?} winding {n:?}");
                assert!(
                    area.abs() > 1e-9 && area.signum() == n[axis].signum(),
                    "{project:?}"
                );
                if n[axis] > 0.0 {
                    toward += 1
                } else {
                    away += 1
                }
            } else {
                assert!(area.abs() < 1e-12, "{project:?} side face {area}");
            }
        }
        assert_eq!((toward, away), (1, 1), "{project:?}");
    }
}

#[test]
fn a_plane_projected_along_its_normal_is_counter_clockwise() {
    for (project, axis, plane_axis) in AXES {
        let mut scene = NativeScene::default();
        let options = PrimitiveOptions::Plane {
            width: 2.0,
            height: 3.0,
            sub_width: 3,
            sub_height: 2,
            axis: plane_axis,
        };
        scene
            .create_with_options(Primitive::Plane, &options, "plane")
            .unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, project).unwrap();
        let faces = face_corners(&scene, shape);
        assert_eq!(faces.len(), 6);
        for face in faces {
            assert!(face_normal(&face)[axis] > 0.0, "{project:?} fixture");
            assert!(uv_signed_area(&face) > 0.0, "{project:?}");
        }
    }
}

/// PROVISIONAL: Maya's pages state no per-axis orientation. Each projection matches the Y-up
/// orthographic view looking at the shape from that axis: (screen right, screen up) in object
/// axes. The signed-area test cannot see a u/v swap or a double flip; this one can.
#[test]
fn uv_directions_follow_the_side_top_and_front_views() {
    let views: [(UvProjectAxis, usize, [f64; 3], [f64; 3]); 3] = [
        (UvProjectAxis::X, 0, [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        (UvProjectAxis::Y, 1, [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        (UvProjectAxis::Z, 2, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    for (project, axis, right, up) in views {
        let mut scene = NativeScene::default();
        scene
            .create_with_options(Primitive::Cube, &box_options(Axis::Y), "box")
            .unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, project).unwrap();
        let faces = face_corners(&scene, shape);
        let face = faces
            .iter()
            .find(|face| face_normal(face)[axis] > 0.5)
            .expect("a face looking along +axis");
        let mut compared = 0;
        for (pa, ua) in face {
            for (pb, ub) in face {
                let delta = [pa[0] - pb[0], pa[1] - pb[1], pa[2] - pb[2]];
                for (component, direction) in [(0, right), (1, up)] {
                    let along = dot(delta, direction);
                    let diff = ua[component] - ub[component];
                    if along.abs() < 1e-9 {
                        assert!(diff.abs() < 1e-9, "{project:?} component {component}");
                    } else {
                        assert!(diff * along > 0.0, "{project:?} component {component}");
                        compared += 1;
                    }
                }
            }
        }
        assert!(compared > 0);
    }
}

#[test]
fn a_projection_with_no_extent_is_refused_and_keeps_the_scene() {
    let (mut scene, shape) = scene_with(Primitive::Plane);
    let before = scene.clone();
    assert_eq!(
        scene.project_uv_planar(shape, UvProjectAxis::X),
        Err(NativeError::DegenerateUvProjection)
    );
    assert!(scene == before);
}

#[test]
fn an_unknown_shape_is_refused() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    let missing = NativeId(shape.0 + 100);
    assert_eq!(
        scene.project_uv_planar(missing, UvProjectAxis::Y),
        Err(NativeError::MissingShape(missing))
    );
    assert_eq!(
        scene.delete_uvs(missing),
        Err(NativeError::MissingShape(missing))
    );
}

#[test]
fn delete_clears_only_that_shape_and_tolerates_no_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.create(Primitive::Cube, "other").unwrap();
    let other = scene.shapes()[1].id();
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene.project_uv_planar(other, UvProjectAxis::Y).unwrap();
    scene.delete_uvs(shape).unwrap();
    assert!(scene.uv_set(shape).is_none());
    assert!(scene.uv_set(other).is_some());
    scene.delete_uvs(shape).unwrap();
}

#[test]
fn removing_a_node_drops_its_uvs_and_stays_valid() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let node = scene.nodes()[0].id();
    scene.remove(node).unwrap();
    assert!(scene.uv_set(shape).is_none());
    scene.validate().unwrap();
}

#[test]
fn extrude_is_refused_while_the_shape_has_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let before = scene.clone();
    assert_eq!(
        scene.extrude_face(shape, 0, 1.0),
        Err(NativeError::UvsBlockTopologyEdit)
    );
    assert!(scene == before);
    scene.delete_uvs(shape).unwrap();
    scene.extrude_face(shape, 0, 1.0).unwrap();
}

#[test]
fn validate_rejects_uvs_that_no_longer_match_the_faces() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene
        .session_uvs
        .sets
        .get_mut(&shape)
        .unwrap()
        .faces
        .pop()
        .unwrap();
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn validate_rejects_non_finite_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene.session_uvs.sets.get_mut(&shape).unwrap().faces[0][0][0] = f64::NAN;
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn authored_writers_refuse_a_scene_with_session_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    assert_eq!(check_content_kind(&scene), Ok(()));
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    assert_eq!(
        check_content_kind(&scene),
        Err(AuthoredIneligible::SessionUvs)
    );
    assert_eq!(
        crate::native_scene::ascii::emit_geometry_prefix(&scene).map(|_| ()),
        Err(crate::native_scene::ascii::AsciiWriteRefusal::SessionUvs)
    );
}

#[test]
fn the_legacy_view_refuses_rather_than_dropping_uvs() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    assert!(scene.supported_view().is_ok());
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    assert_eq!(
        scene.supported_view(),
        Err(NativeError::UnsupportedView("session UVs"))
    );
}

// `validate` runs on every structural edit, so it must visit each shape once however many shapes
// hold UVs, not once per set. Counting visits pins that without a wall-clock assertion.
#[test]
fn validate_visits_each_shape_once_however_many_hold_uvs() {
    for count in [1usize, 64] {
        let mut scene = NativeScene::default();
        for index in 0..count {
            scene.create(Primitive::Cube, &format!("c{index}")).unwrap();
        }
        let shapes: Vec<NativeId> = scene.shapes().iter().map(|shape| shape.id()).collect();
        for shape in shapes {
            scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
        }
        let before = SHAPE_VISITS.with(std::cell::Cell::get);
        scene.validate().unwrap();
        let visits = SHAPE_VISITS.with(std::cell::Cell::get) - before;
        assert_eq!(visits, count, "{count} shapes with UVs");
    }
}

#[test]
fn validate_rejects_a_uv_set_whose_shape_is_gone() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    let set = scene.session_uvs.sets[&shape].clone();
    let orphan = NativeId(shape.0 + 1_000);
    scene.session_uvs.sets.insert(orphan, set);
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn remove_many_prunes_only_the_removed_nodes_uvs_and_stays_valid() {
    let mut scene = NativeScene::default();
    let gone = scene.create(Primitive::Cube, "gone").unwrap();
    let kept = scene.create(Primitive::Cube, "kept").unwrap();
    let ids: Vec<NativeId> = scene.shapes().iter().map(|shape| shape.id()).collect();
    for shape in &ids {
        scene.project_uv_planar(*shape, UvProjectAxis::Z).unwrap();
    }
    let kept_set = scene.uv_set(ids[1]).unwrap().clone();
    scene.remove_many(&[gone]).unwrap();
    assert!(scene.uv_set(ids[0]).is_none());
    assert_eq!(scene.uv_set(ids[1]), Some(&kept_set));
    assert_eq!(scene.validate(), Ok(()));
    scene.remove_many(&[kept]).unwrap();
    assert!(scene.session_uvs.is_empty());
}

#[test]
fn projection_and_delete_are_refused_under_a_restriction_and_change_nothing() {
    for on_shape in [false, true] {
        let (mut scene, shape) = scene_with(Primitive::Cube);
        scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
        if on_shape {
            scene.shapes[0].restrictions.push("locked".into());
        } else {
            scene.nodes[0].restrictions.push("locked".into());
        }
        let before = scene.clone();
        assert_eq!(
            scene.project_uv_planar(shape, UvProjectAxis::X),
            Err(NativeError::UnsupportedEdit)
        );
        assert_eq!(scene.delete_uvs(shape), Err(NativeError::UnsupportedEdit));
        assert!(scene == before);
    }
}

#[test]
fn projecting_an_instanced_shape_is_accepted_and_the_set_is_per_shape() {
    let mut scene = NativeScene::default();
    scene.create(Primitive::Cube, "a").unwrap();
    scene.create(Primitive::Cube, "b").unwrap();
    let shape = scene.shapes()[0].id();
    let other_transform = scene.nodes()[1].transform_id;
    scene.dag.push(crate::native_scene::DagEdge {
        parent: Some(other_transform),
        child: shape,
    });
    assert_eq!(
        scene.dag.iter().filter(|edge| edge.child == shape).count(),
        2
    );
    scene.project_uv_planar(shape, UvProjectAxis::Z).unwrap();
    assert_eq!(scene.validate(), Ok(()));
    // UVs are keyed by shape and projected in object space, so every instance reads the one set.
    assert_eq!(scene.session_uvs.sets.len(), 1);
    assert!(scene.uv_set(shape).is_some());
}

fn cube_mesh() -> NativeMesh {
    let (scene, shape) = scene_with(Primitive::Cube);
    scene.shape(shape).unwrap().mesh().clone()
}

fn corners_of(mesh: &NativeMesh, uv: [f64; 2]) -> Vec<Vec<[f64; 2]>> {
    mesh.faces()
        .iter()
        .map(|f| vec![uv; f.outer().len()])
        .collect()
}

#[test]
fn planar_values_are_exactly_f32() {
    for (axis, _, plane_axis) in AXES {
        let mut scene = NativeScene::default();
        let options = PrimitiveOptions::Plane {
            width: 3.0,
            height: 7.0,
            sub_width: 3,
            sub_height: 3,
            axis: plane_axis,
        };
        scene
            .create_with_options(Primitive::Plane, &options, "p")
            .unwrap();
        let shape = scene.shapes()[0].id();
        scene.project_uv_planar(shape, axis).unwrap();
        let values: Vec<f64> = scene
            .uv_set(shape)
            .unwrap()
            .faces()
            .iter()
            .flatten()
            .flatten()
            .copied()
            .collect();
        assert!(
            values.iter().any(|v| (*v * 3.0).fract() != 0.0),
            "{axis:?} fixture is not fractional"
        );
        for v in values {
            assert_eq!(f64::from(v as f32), v, "{axis:?}");
        }
    }
}

#[test]
fn planar_projection_of_irrational_extent_is_quantised_and_valid() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene
        .create_with_options(
            Primitive::Plane,
            &PrimitiveOptions::Plane {
                width: 3.0,
                height: 7.0,
                sub_width: 3,
                sub_height: 3,
                axis: Axis::Y,
            },
            "p",
        )
        .unwrap();
    let plane = scene.shapes()[1].id();
    let _ = shape;
    scene.project_uv_planar(plane, UvProjectAxis::Y).unwrap();
    assert!(
        scene
            .uv_set(plane)
            .unwrap()
            .faces()
            .iter()
            .flatten()
            .flatten()
            .any(|v| (*v * 3.0).fract() != 0.0)
    );
    assert_eq!(scene.validate(), Ok(()));
}

#[test]
fn from_corners_rounds_to_the_nearest_f32() {
    let mesh = cube_mesh();
    let set = UvSet::from_corners(&mesh, corners_of(&mesh, [0.1, 1.0 / 3.0])).unwrap();
    let uv = set.faces()[0][0];
    assert_eq!(uv, [f64::from(0.1_f32), f64::from(1.0_f32 / 3.0)]);
    assert_ne!(uv[0], 0.1);
    assert_eq!(set.corner_count(), 24);
}

#[test]
fn from_corners_refuses_bad_shapes_and_values() {
    let mesh = cube_mesh();
    let mut short = corners_of(&mesh, [0.0, 0.0]);
    short.pop();
    assert_eq!(
        UvSet::from_corners(&mesh, short),
        Err(NativeError::InvalidSessionUvs)
    );
    let mut wrong = corners_of(&mesh, [0.0, 0.0]);
    wrong[0].pop();
    assert_eq!(
        UvSet::from_corners(&mesh, wrong),
        Err(NativeError::InvalidSessionUvs)
    );
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1e300, -1e300] {
        for slot in 0..2 {
            let mut uv = [0.5, 0.5];
            uv[slot] = bad;
            assert_eq!(
                UvSet::from_corners(&mesh, corners_of(&mesh, uv)),
                Err(NativeError::InvalidSessionUvs),
                "{bad}"
            );
        }
    }
}

#[test]
fn validate_rejects_a_finite_value_that_is_not_f32_exact() {
    let (mut scene, shape) = scene_with(Primitive::Cube);
    scene.project_uv_planar(shape, UvProjectAxis::Y).unwrap();
    scene.session_uvs.sets.get_mut(&shape).unwrap().faces[0][0][1] = 0.1;
    assert_eq!(scene.validate(), Err(NativeError::InvalidSessionUvs));
}

#[test]
fn from_corners_refuses_a_face_with_a_hole() {
    let mut mesh = NativeMesh::polygon_fixture(
        vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [2.0, 2.0, 0.0],
            [4.0, 2.0, 0.0],
            [4.0, 4.0, 0.0],
        ],
        &[vec![0, 1, 2, 3]],
    );
    mesh.add_hole_fixture(&[4, 5, 6]);
    assert_eq!(
        UvSet::from_corners(&mesh, vec![vec![[0.0, 0.0]; 4]]),
        Err(NativeError::UnsupportedUvTopology)
    );
}
