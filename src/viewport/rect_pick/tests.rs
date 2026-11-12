use super::*;
use crate::native_scene::{NativeScene, NativeTransform, test_fixtures};
use crate::scene::Primitive;
use crate::view_scene::{ViewError, ViewScene};
use crate::viewport::{Camera, InvalidCameraInput};

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

fn rect(min: [f32; 2], max: [f32; 2]) -> NdcRect {
    NdcRect::new(v(min[0], min[1]), v(max[0], max[1])).unwrap()
}

/// Counter-clockwise right triangle with corners (0,0), (1,0), (0,1).
const TRI: [Vec2; 3] = [
    Vec2::new(0.0, 0.0),
    Vec2::new(1.0, 0.0),
    Vec2::new(0.0, 1.0),
];

fn hits(polygon: &[Vec2], rect: NdcRect) -> bool {
    polygon_overlaps_rect(polygon, rect)
}

#[test]
fn rect_strictly_inside_a_triangle_hits_it_without_containing_a_vertex() {
    assert!(hits(&TRI, rect([0.1, 0.1], [0.2, 0.2])));
}

#[test]
fn triangle_inside_the_rect_hits() {
    assert!(hits(&TRI, rect([-0.5, -0.5], [0.9, 0.9])));
}

#[test]
fn edge_crossing_the_rect_with_no_vertex_inside_hits() {
    // The hypotenuse passes through the rect; no triangle corner lies in it.
    assert!(hits(&TRI, rect([0.4, 0.4], [0.7, 0.7])));
}

#[test]
fn rect_separated_only_by_the_hypotenuse_misses_although_the_boxes_overlap() {
    assert!(!hits(&TRI, rect([0.6, 0.6], [0.9, 0.9])));
}

#[test]
fn rect_corner_exactly_on_a_vertex_or_on_an_edge_hits() {
    assert!(hits(&TRI, rect([1.0, 0.0], [1.0, 0.0])));
    assert!(hits(&TRI, rect([0.5, 0.5], [0.9, 0.9])));
    assert!(hits(&TRI, rect([-0.5, -0.5], [0.0, 0.0])));
}

#[test]
fn rect_touching_only_along_a_shared_edge_hits() {
    assert!(hits(&TRI, rect([-0.5, 0.0], [0.0, 1.0])));
    assert!(hits(&TRI, rect([0.0, -0.5], [1.0, 0.0])));
}

#[test]
fn rect_just_outside_misses() {
    assert!(!hits(&TRI, rect([-0.5, 0.0], [-0.001, 1.0])));
    assert!(!hits(&TRI, rect([0.0, -0.5], [1.0, -0.001])));
}

#[test]
fn degenerate_rects_act_as_a_line_or_a_point_test() {
    assert!(hits(&TRI, rect([0.2, 0.2], [0.2, 0.2])));
    assert!(!hits(&TRI, rect([0.8, 0.8], [0.8, 0.8])));
    assert!(hits(&TRI, rect([0.2, -0.5], [0.2, 0.5])));
    assert!(hits(&TRI, rect([-0.5, 0.2], [0.5, 0.2])));
    assert!(!hits(&TRI, rect([0.2, -0.5], [0.2, -0.1])));
}

#[test]
fn clockwise_winding_hits_the_same_as_counter_clockwise() {
    let reversed = [TRI[2], TRI[1], TRI[0]];
    for r in [
        rect([0.1, 0.1], [0.2, 0.2]),
        rect([0.4, 0.4], [0.7, 0.7]),
        rect([0.6, 0.6], [0.9, 0.9]),
    ] {
        assert_eq!(hits(&TRI, r), hits(&reversed, r));
    }
}

#[test]
fn collinear_triangle_is_skipped_even_when_it_crosses_the_rect() {
    let line = [v(-1.0, -1.0), v(0.0, 0.0), v(1.0, 1.0)];
    assert!(!hits(&line, rect([-0.5, -0.5], [0.5, 0.5])));
}

#[test]
fn fewer_than_three_vertices_never_hit() {
    let whole = rect([-1.0, -1.0], [1.0, 1.0]);
    assert!(!hits(&[], whole));
    assert!(!hits(&[v(0.0, 0.0)], whole));
    assert!(!hits(&[v(0.0, 0.0), v(0.5, 0.5)], whole));
}

#[test]
fn the_whole_viewport_rect_hits_any_visible_triangle() {
    assert!(hits(&TRI, rect([-1.0, -1.0], [1.0, 1.0])));
}

#[test]
fn ndc_rect_new_normalises_corner_order() {
    let r = NdcRect::new(v(0.5, -0.25), v(-0.5, 0.75)).unwrap();
    assert_eq!((r.min(), r.max()), (v(-0.5, -0.25), v(0.5, 0.75)));
}

#[test]
fn ndc_rect_new_accepts_the_viewport_edges_and_rejects_the_rest() {
    assert!(NdcRect::new(v(-1.0, -1.0), v(1.0, 1.0)).is_ok());
    for bad in [
        (v(f32::NAN, 0.0), v(0.0, 0.0)),
        (v(0.0, 0.0), v(0.0, f32::INFINITY)),
        (v(0.0, 0.0), v(1.0_f32.next_up(), 0.0)),
        (v((-1.0_f32).next_down(), 0.0), v(0.0, 0.0)),
        (v(0.0, 1.5), v(0.0, 0.0)),
    ] {
        assert_eq!(NdcRect::new(bad.0, bad.1), Err(InvalidRect), "{bad:?}");
    }
}

#[test]
fn ndc_rect_clamped_clips_to_the_viewport_and_rejects_non_finite() {
    let r = NdcRect::clamped(v(-3.0, 0.5), v(0.25, 7.0)).unwrap();
    assert_eq!((r.min(), r.max()), (v(-1.0, 0.5), v(0.25, 1.0)));
    let outside = NdcRect::clamped(v(2.0, 2.0), v(3.0, 3.0)).unwrap();
    assert_eq!((outside.min(), outside.max()), (v(1.0, 1.0), v(1.0, 1.0)));
    assert_eq!(
        NdcRect::clamped(v(f32::NAN, 0.0), v(0.0, 0.0)),
        Err(InvalidRect)
    );
    assert_eq!(
        NdcRect::clamped(v(0.0, 0.0), v(f32::NEG_INFINITY, 0.0)),
        Err(InvalidRect)
    );
}

fn clip_of(triangle: [Vec4; 3]) -> Vec<Vec4> {
    let (mut scratch, mut out) = (vec![Vec4::ONE; 9], vec![Vec4::ONE; 9]);
    clip_triangle_depth(triangle, &mut scratch, &mut out);
    out
}

fn clip(x: f32, y: f32, z: f32, w: f32) -> Vec4 {
    Vec4::new(x, y, z, w)
}

#[test]
fn clip_keeps_a_triangle_inside_the_depth_range_unchanged() {
    let tri = [
        clip(0.0, 0.0, 0.5, 1.0),
        clip(1.0, 0.0, 0.5, 1.0),
        clip(0.0, 1.0, 0.5, 1.0),
    ];
    assert_eq!(clip_of(tri), tri);
}

#[test]
fn clip_drops_a_triangle_behind_the_near_plane_or_beyond_the_far_plane() {
    let behind = [
        clip(0.0, 0.0, -0.5, 1.0),
        clip(1.0, 0.0, -0.1, 1.0),
        clip(0.0, 1.0, -0.2, 1.0),
    ];
    assert!(clip_of(behind).is_empty());
    let beyond = [
        clip(0.0, 0.0, 2.0, 1.0),
        clip(1.0, 0.0, 1.5, 1.0),
        clip(0.0, 1.0, 3.0, 1.0),
    ];
    assert!(clip_of(beyond).is_empty());
}

#[test]
fn clip_cuts_a_triangle_straddling_the_near_plane_at_z_zero() {
    let tri = [
        clip(0.0, 0.0, -1.0, 1.0),
        clip(1.0, 0.0, 1.0, 1.0),
        clip(0.0, 1.0, 0.5, 1.0),
    ];
    let polygon = clip_of(tri);
    assert!(polygon.len() >= 3);
    for p in &polygon {
        assert!(p.z >= 0.0 && p.z <= p.w, "{p:?}");
    }
    assert!(polygon.iter().any(|p| p.z == 0.0));
}

#[test]
fn clip_cuts_a_triangle_straddling_the_far_plane_at_z_equal_w() {
    let tri = [
        clip(0.0, 0.0, 0.5, 1.0),
        clip(1.0, 0.0, 2.0, 1.0),
        clip(0.0, 1.0, 0.5, 1.0),
    ];
    let polygon = clip_of(tri);
    assert!(polygon.len() >= 3);
    for p in &polygon {
        assert!(p.z >= 0.0 && p.z <= p.w, "{p:?}");
    }
    assert!(polygon.iter().any(|p| p.z == p.w));
}

const ASPECT: f32 = 1.5;
const WHOLE: [[f32; 2]; 2] = [[-1.0, -1.0], [1.0, 1.0]];

fn view_of(scene: &NativeScene) -> ViewScene {
    ViewScene::from_native(scene, 1 << 28).unwrap()
}

fn pick(camera: &Camera, view: &ViewScene, bounds: [[f32; 2]; 2]) -> Vec<usize> {
    camera
        .pick_rect(view, rect(bounds[0], bounds[1]), ASPECT)
        .unwrap()
}

/// Cubes named in creation order, translated by `x` along the world X axis.
fn cubes_at(xs: &[f64]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (index, x) in xs.iter().enumerate() {
        let node = scene.create(Primitive::Cube, &format!("c{index}")).unwrap();
        scene
            .set_transform(
                node,
                NativeTransform {
                    translation: [*x, 0.0, 0.0],
                    ..Default::default()
                },
            )
            .unwrap();
    }
    scene
}

/// The NDC of the projected centre of instance `index`.
fn centre_of(camera: &Camera, view: &ViewScene, index: usize) -> Vec2 {
    let vertices = view.instances()[index].world_vertices();
    let centre = vertices.iter().copied().sum::<glam::Vec3>() / vertices.len() as f32;
    camera.project(centre, ASPECT).unwrap().unwrap().truncate()
}

fn around(centre: Vec2, half: f32) -> [[f32; 2]; 2] {
    [
        [centre.x - half, centre.y - half],
        [centre.x + half, centre.y + half],
    ]
}

#[test]
fn whole_viewport_selects_every_visible_instance_in_instance_order_without_duplicates() {
    let camera = Camera::default();
    let view = view_of(&cubes_at(&[-3.0, 0.0, 3.0]));
    assert_eq!(view.instances().len(), 3);
    assert_eq!(pick(&camera, &view, WHOLE), [0, 1, 2]);
}

#[test]
fn a_rect_over_empty_space_selects_nothing() {
    let camera = Camera::default();
    let view = view_of(&cubes_at(&[0.0]));
    assert!(pick(&camera, &view, [[0.9, 0.9], [1.0, 1.0]]).is_empty());
}

#[test]
fn a_rect_around_one_cube_selects_only_that_cube() {
    let camera = Camera::default();
    let view = view_of(&cubes_at(&[-3.0, 0.0, 3.0]));
    for index in 0..3 {
        let bounds = around(centre_of(&camera, &view, index), 0.01);
        assert_eq!(pick(&camera, &view, bounds), [index]);
    }
}

#[test]
fn a_rect_strictly_inside_a_face_selects_it_with_no_vertex_inside() {
    let camera = Camera::default();
    let view = view_of(&cubes_at(&[0.0]));
    let bounds = around(centre_of(&camera, &view, 0), 1e-4);
    assert_eq!(pick(&camera, &view, bounds), [0]);
}

#[test]
fn inverted_corners_select_the_same_instances() {
    let camera = Camera::default();
    let view = view_of(&cubes_at(&[-3.0, 3.0]));
    let bounds = around(centre_of(&camera, &view, 1), 0.02);
    let flipped =
        NdcRect::new(v(bounds[1][0], bounds[1][1]), v(bounds[0][0], bounds[0][1])).unwrap();
    assert_eq!(camera.pick_rect(&view, flipped, ASPECT).unwrap(), [1]);
}

#[test]
fn backfacing_and_mirrored_surfaces_are_selected_without_culling() {
    let camera = Camera::default();
    for scale in [
        [1.0, 1.0, 1.0],
        [1.0, -1.0, 1.0],
        [-1.0, 1.0, 1.0],
        [-1.0, -1.0, -1.0],
    ] {
        let mut scene = NativeScene::default();
        let node = scene.create(Primitive::Plane, "plane").unwrap();
        scene
            .set_transform(
                node,
                NativeTransform {
                    scale,
                    ..Default::default()
                },
            )
            .unwrap();
        let view = view_of(&scene);
        let bounds = around(centre_of(&camera, &view, 0), 1e-3);
        assert_eq!(pick(&camera, &view, bounds), [0], "scale {scale:?}");
    }
}

#[test]
fn two_instances_of_one_shape_are_two_distinct_indices() {
    let (scene, _, _, _) = test_fixtures::shared_shape();
    let view = view_of(&scene);
    assert_eq!(view.instances().len(), 2);
    assert_eq!(pick(&Camera::default(), &view, WHOLE), [0, 1]);
}

#[test]
fn hidden_instances_are_not_in_the_view_so_they_are_never_selected() {
    let (mut scene, _) = test_fixtures::cube();
    test_fixtures::hide_first_shape(&mut scene);
    let view = view_of(&scene);
    assert!(pick(&Camera::default(), &view, WHOLE).is_empty());
}

fn scaled(primitive: Primitive, scale: f64, x: f64) -> NativeScene {
    let mut scene = NativeScene::default();
    let node = scene.create(primitive, "big").unwrap();
    scene
        .set_transform(
            node,
            NativeTransform {
                translation: [x, 0.0, 0.0],
                scale: [scale; 3],
                ..Default::default()
            },
        )
        .unwrap();
    scene
}

#[test]
fn an_instance_wholly_behind_the_camera_is_never_selected() {
    let mut camera = Camera::default();
    let view = view_of(&cubes_at(&[0.0]));
    let back = camera.eye() / camera.distance();
    camera.set_pose(-back * 100.0, 8.0, 0.6, 0.4).unwrap();
    assert!(
        camera.eye().dot(back) < 0.0,
        "the cube must be behind the eye"
    );
    assert!(pick(&camera, &view, WHOLE).is_empty());
}

#[test]
fn an_instance_wholly_beyond_the_far_plane_is_never_selected() {
    let camera = Camera::default();
    let view = view_of(&scaled(Primitive::Cube, 1.0, 1_000_000.0));
    assert!(pick(&camera, &view, WHOLE).is_empty());
}

#[test]
fn an_instance_straddling_the_near_or_far_plane_is_selected() {
    let camera = Camera::default();
    // The eye is inside this cube, so its faces cross the near plane.
    let inside = view_of(&scaled(Primitive::Cube, 100.0, 0.0));
    assert_eq!(pick(&camera, &inside, WHOLE), [0]);
    // A ground plane running past the far plane (10000).
    let ground = view_of(&scaled(Primitive::Plane, 100_000.0, 0.0));
    assert_eq!(pick(&camera, &ground, WHOLE), [0]);
}

#[test]
fn non_finite_camera_input_is_an_error_and_selects_nothing() {
    let view = view_of(&cubes_at(&[0.0]));
    let camera = Camera::default();
    let whole = rect(WHOLE[0], WHOLE[1]);
    for aspect in [f32::NAN, 0.0, -1.0, f32::INFINITY] {
        assert_eq!(
            camera.pick_rect(&view, whole, aspect),
            Err(ViewError::Camera(InvalidCameraInput))
        );
    }
    let broken = Camera {
        distance: f32::NAN,
        ..Camera::default()
    };
    assert_eq!(
        broken.pick_rect(&view, whole, ASPECT),
        Err(ViewError::Camera(InvalidCameraInput))
    );
    let huge = Camera {
        target: glam::Vec3::splat(f32::MAX),
        ..Camera::default()
    };
    assert_eq!(
        huge.pick_rect(&view, whole, ASPECT),
        Err(ViewError::Camera(InvalidCameraInput))
    );
}

/// A seeded sweep of points: wherever the ray pick finds a surface, the degenerate rect at that
/// point selects the instance it belongs to.
fn assert_point_rects_agree_with_pick_view(view: &ViewScene) {
    let camera = Camera::default();
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    };
    let mut hit_count = 0;
    for _ in 0..2000 {
        let point = v(next(), next());
        let Some(hit) = camera.pick_view(view, point, ASPECT).unwrap() else {
            continue;
        };
        hit_count += 1;
        let expected = view
            .instances()
            .iter()
            .position(|instance| instance.path() == hit.path.as_slice())
            .unwrap();
        let selected = camera
            .pick_rect(view, NdcRect::new(point, point).unwrap(), ASPECT)
            .unwrap();
        assert!(
            selected.contains(&expected),
            "{point:?} -> {selected:?} vs {expected}"
        );
    }
    assert!(hit_count > 20, "the sweep must hit something: {hit_count}");
}

fn transformed_cubes(transforms: &[NativeTransform]) -> NativeScene {
    let mut scene = NativeScene::default();
    for (index, transform) in transforms.iter().enumerate() {
        let node = scene.create(Primitive::Cube, &format!("c{index}")).unwrap();
        scene.set_transform(node, *transform).unwrap();
    }
    scene
}

#[test]
fn a_point_rect_selects_at_least_the_instance_the_ray_pick_hits() {
    assert_point_rects_agree_with_pick_view(&view_of(&cubes_at(&[-4.0, -1.0, 2.0, 5.0])));
}

#[test]
fn the_agreement_sweep_holds_for_rotated_and_mirrored_instances() {
    let at = |x: f64, rotation: [f64; 3], scale: [f64; 3]| NativeTransform {
        translation: [x, 0.0, 0.0],
        rotation,
        scale,
    };
    let rotated = transformed_cubes(&[
        at(-2.0, [20.0, 35.0, -15.0], [1.0; 3]),
        at(1.5, [70.0, -10.0, 40.0], [1.0, 2.0, 0.5]),
    ]);
    assert_point_rects_agree_with_pick_view(&view_of(&rotated));
    for scale in [[-1.0, 1.0, 1.0], [-1.0, -1.0, 1.0], [-1.5, -1.0, -0.5]] {
        let mirrored = transformed_cubes(&[
            at(-2.0, [0.0; 3], scale),
            at(1.5, [30.0, 20.0, 10.0], scale),
        ]);
        assert_point_rects_agree_with_pick_view(&view_of(&mirrored));
    }
}

#[test]
fn the_agreement_sweep_holds_for_nested_and_instanced_paths() {
    let (chain, ..) = test_fixtures::instanced_chain();
    let chain = view_of(&chain);
    assert!(chain.instances().iter().any(|i| i.path().len() > 2));
    assert_point_rects_agree_with_pick_view(&chain);
    let (grouped, ..) = test_fixtures::grouped_shape();
    assert_point_rects_agree_with_pick_view(&view_of(&grouped));
}

// Slivers whose vertices are far from the NDC origin, as a triangle crossing the near plane
// gives. Absolute-coordinate area terms cancel to zero or flip sign here; the true areas are
// about 5e-3.
const FAR_ZERO_AREA: [Vec2; 3] = [
    Vec2::new(380.8495, 911.8197),
    Vec2::new(-380.74948, -911.61975),
    Vec2::new(-22.772835, -54.54303),
];
const FAR_FLIPPED_AREA: [Vec2; 3] = [
    Vec2::new(-379.0048, 214.43462),
    Vec2::new(379.1048, -214.2346),
    Vec2::new(-60.885017, 34.55541),
];

#[test]
fn far_sliver_with_cancelling_area_terms_still_hits_a_rect_it_crosses() {
    assert!(hits(&FAR_ZERO_AREA, rect([-0.5, -0.5], [0.5, 0.5])));
}

#[test]
fn far_sliver_with_a_flipped_area_sign_hits_a_crossed_rect_and_misses_a_clear_one() {
    assert!(hits(&FAR_FLIPPED_AREA, rect([-0.5, -0.5], [0.5, 0.5])));
    // The sliver runs along y = -0.57 x + 0.01 through the origin; this rect is far above it
    // yet inside its bounding box.
    assert!(!hits(&FAR_FLIPPED_AREA, rect([0.8, 0.8], [1.0, 1.0])));
}

#[test]
fn far_sliver_hit_does_not_depend_on_winding() {
    let reversed = [
        FAR_FLIPPED_AREA[2],
        FAR_FLIPPED_AREA[1],
        FAR_FLIPPED_AREA[0],
    ];
    assert!(hits(&reversed, rect([-0.5, -0.5], [0.5, 0.5])));
    assert!(!hits(&reversed, rect([0.8, 0.8], [1.0, 1.0])));
}

#[test]
fn exactly_collinear_far_triangle_is_still_skipped() {
    let line = [v(-1000.0, -2000.0), v(0.0, 0.0), v(1000.0, 2000.0)];
    assert!(!hits(&line, rect([-0.5, -0.5], [0.5, 0.5])));
}

/// Independent f64 reference: per-edge cross-product half-plane test plus a bounding-box test.
fn reference_hits(polygon: &[Vec2], rect: NdcRect) -> bool {
    let p: Vec<[f64; 2]> = polygon.iter().map(|q| [q.x as f64, q.y as f64]).collect();
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let area = cross(p[0], p[1], p[2]);
    if area == 0.0 {
        return false;
    }
    let (min, max) = (rect.min(), rect.max());
    let corners = [
        [min.x as f64, min.y as f64],
        [max.x as f64, min.y as f64],
        [max.x as f64, max.y as f64],
        [min.x as f64, max.y as f64],
    ];
    // Overlap unless the rect is strictly outside one edge's half-plane or the boxes are apart.
    for i in 0..3 {
        let (a, b) = (p[i], p[(i + 1) % 3]);
        if corners
            .iter()
            .all(|c| cross(a, b, *c) * area.signum() < 0.0)
        {
            return false;
        }
    }
    let lo = [
        p.iter().map(|q| q[0]).fold(f64::MAX, f64::min),
        p.iter().map(|q| q[1]).fold(f64::MAX, f64::min),
    ];
    let hi = [
        p.iter().map(|q| q[0]).fold(f64::MIN, f64::max),
        p.iter().map(|q| q[1]).fold(f64::MIN, f64::max),
    ];
    !(hi[0] < min.x as f64 || lo[0] > max.x as f64 || hi[1] < min.y as f64 || lo[1] > max.y as f64)
}

#[test]
fn random_far_slivers_agree_with_the_f64_reference() {
    let mut state = 99_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as f64 / 2147483648.0
    };
    for case in 0..4000 {
        let m = [1e2, 1e3, 1e4][case % 3];
        let (dx, dy) = (next() * 2.0 - 1.0, next() * 2.0 - 1.0);
        let (a, b) = ((-m * dx + 0.1, -m * dy), (m * dx, m * dy + 0.2));
        let (t, off) = (next() * 0.5 + 0.25, (next() - 0.5) * 0.02);
        let c = (
            a.0 + (b.0 - a.0) * t - (b.1 - a.1) * off / m,
            a.1 + (b.1 - a.1) * t + (b.0 - a.0) * off / m,
        );
        let tri = [a, b, c].map(|q| v(q.0 as f32, q.1 as f32));
        let (rx, ry) = (next() as f32 * 1.6 - 0.8, next() as f32 * 1.6 - 0.8);
        let r = rect([rx, ry], [rx + 0.1, ry + 0.1]);
        assert_eq!(
            hits(&tri, r),
            reference_hits(&tri, r),
            "case {case}: {tri:?}"
        );
    }
}
