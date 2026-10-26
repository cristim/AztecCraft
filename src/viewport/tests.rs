use super::*;
use crate::native_scene::NativeScene;
use crate::native_scene::test_fixtures;
use crate::view_scene::ViewScene;

fn frame(pointer: Vec2, pixels: [u32; 2], ppp: f32) -> Result<PickFrame, ViewError> {
    Camera::default().pick_frame(pointer, pixels, ppp)
}

fn refused(pointer: Vec2, pixels: [u32; 2], ppp: f32) -> bool {
    matches!(
        frame(pointer, pixels, ppp),
        Err(ViewError::Camera(InvalidCameraInput))
    )
}

#[test]
fn pick_frame_accepts_the_viewport_rectangle_edges_inclusively() {
    let accepted = frame(Vec2::new(200.0, 100.0), [200, 100], 1.0).unwrap();
    assert_eq!(accepted.size, Vec2::new(200.0, 100.0));
    assert_eq!(accepted.pointer, Vec2::new(200.0, 100.0));
    assert_eq!(accepted.aspect, 2.0);
    let origin = frame(Vec2::ZERO, [200, 100], 1.0).unwrap();
    assert_eq!(origin.pointer, Vec2::ZERO);
    let scaled = frame(Vec2::new(100.0, 50.0), [200, 100], 2.0).unwrap();
    assert_eq!(scaled.pointer, Vec2::new(200.0, 100.0));
}

#[test]
fn pick_frame_rejects_pointers_outside_the_viewport() {
    let past = 200.0_f32.next_up();
    let below = -f32::MIN_POSITIVE;
    assert!(refused(Vec2::new(past, 50.0), [200, 100], 1.0));
    assert!(refused(
        Vec2::new(100.0, 100.0_f32.next_up()),
        [200, 100],
        1.0
    ));
    assert!(refused(Vec2::new(below, 50.0), [200, 100], 1.0));
    assert!(refused(Vec2::new(100.0, below), [200, 100], 1.0));
    assert!(refused(Vec2::new(-1.0, 50.0), [200, 100], 1.0));
    assert!(refused(Vec2::new(100.0, -1.0), [200, 100], 1.0));
    assert!(refused(Vec2::new(101.0, 50.0), [200, 100], 2.0));
}

#[test]
fn pick_frame_rejects_empty_and_non_finite_input() {
    let pointer = Vec2::new(10.0, 10.0);
    assert!(refused(pointer, [0, 100], 1.0));
    assert!(refused(pointer, [200, 0], 1.0));
    assert!(refused(Vec2::ZERO, [200, 0], 1.0));
    assert!(refused(Vec2::ZERO, [0, 100], 1.0));
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(refused(Vec2::new(bad, 10.0), [200, 100], 1.0));
        assert!(refused(Vec2::new(10.0, bad), [200, 100], 1.0));
        assert!(refused(pointer, [200, 100], bad));
    }
    assert!(refused(pointer, [200, 100], 0.0));
    assert!(refused(pointer, [200, 100], -1.0));
}

const SIZE: [u32; 2] = [800, 600];

fn camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 45_f32.to_radians(), 0.01, 100.0).unwrap()
}

/// World units per screen pixel at the z = 0 plane for `camera()`.
fn unit(pixels: f32) -> f32 {
    pixels * 5.0 * (22.5_f32.to_radians()).tan() / 300.0
}

fn world(x_px: f32, y_px: f32, z: f32) -> [f64; 3] {
    [unit(x_px).into(), unit(y_px).into(), z.into()]
}

fn pixel_of(point: Vec3) -> Vec2 {
    let ndc = camera().project(point, 800.0 / 600.0).unwrap().unwrap();
    Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * Vec2::new(800.0, 600.0)
}

struct Fixture {
    native: NativeScene,
    view: ViewScene,
    owner: NodeId,
    path: Vec<NativeId>,
}

impl Fixture {
    fn new(points: Vec<[f64; 3]>, loops: &[Vec<usize>], loose_edges: &[[usize; 2]]) -> Self {
        let (mut native, owner) = test_fixtures::polygon_shape(points, loops);
        for &endpoints in loose_edges {
            test_fixtures::add_loose_edge_unvalidated(&mut native, endpoints);
        }
        let view = ViewScene::from_native(&native, 1 << 28).unwrap();
        let path = vec![native.node(owner).unwrap().transform_id()];
        Self {
            native,
            view,
            owner,
            path,
        }
    }
    fn vertex_at(&self, pointer: Vec2) -> Result<Option<usize>, ViewError> {
        camera()
            .pick_vertex(
                &self.view,
                Some((self.owner, &self.path)),
                pointer,
                SIZE,
                1.0,
            )
            .map(|target| {
                target.map(|target| {
                    assert!(self.native.resolve_vertex_target(&target).is_ok());
                    target.vertex_index
                })
            })
    }
    fn edge_at(&self, pointer: Vec2) -> Result<Option<usize>, ViewError> {
        camera()
            .pick_edge(
                &self.view,
                Some((self.owner, &self.path)),
                pointer,
                SIZE,
                1.0,
            )
            .map(|target| {
                target.map(|target| {
                    assert!(self.native.resolve_edge_target(&target).is_ok());
                    target.edge_index
                })
            })
    }
    fn vertex(&self, index: usize) -> Vec2 {
        pixel_of(self.view.instances()[0].world_vertices()[index])
    }
}

/// World point that projects to the given pixel offset from the screen center at height `z`.
fn world_at(x_px: f32, y_px: f32, z: f32) -> [f64; 3] {
    let scale = (5.0 - z) / 5.0;
    [
        (unit(x_px) * scale).into(),
        (unit(y_px) * scale).into(),
        z.into(),
    ]
}

fn near(a: f32, b: f32) -> Vec2 {
    Vec2::new(a, b)
}

#[test]
fn depth_tolerances_are_pinned_independently() {
    assert_eq!(FACE_CENTER_DEPTH_TOLERANCE, 1e-4);
    assert_eq!(VERTEX_MARKER_DEPTH_TOLERANCE, 6e-3);
}

#[test]
fn vertex_pick_uses_the_click_box_and_reaches_loose_vertices() {
    let points = vec![
        world(-100.0, -100.0, 0.0),
        world(100.0, -100.0, 0.0),
        world(0.0, 100.0, 0.0),
        world(-250.0, 0.0, 0.0),
    ];
    let fixture = Fixture::new(points, &[vec![0, 1, 2]], &[]);
    let loose = fixture.vertex(3);
    assert_eq!(fixture.vertex_at(loose), Ok(Some(3)));
    assert_eq!(fixture.vertex_at(loose + near(4.9, -4.9)), Ok(Some(3)));
    assert_eq!(fixture.vertex_at(loose + near(5.2, 0.0)), Ok(None));
    assert_eq!(fixture.vertex_at(loose + near(0.0, -5.2)), Ok(None));
    assert_eq!(fixture.vertex_at(fixture.vertex(2)), Ok(Some(2)));
}

#[test]
fn vertex_pick_prefers_the_nearest_to_the_pointer_then_the_lowest_index() {
    let points = vec![
        world(-100.0, -100.0, 0.0),
        world(100.0, -100.0, 0.0),
        world(0.0, 100.0, 0.0),
        world(-250.0, 0.0, 0.0),
        world(-244.0, 0.0, 0.0),
        world(250.0, 0.0, 0.0),
        world(250.0, 0.0, 0.0),
    ];
    let fixture = Fixture::new(points, &[vec![0, 1, 2]], &[]);
    let (a, b) = (fixture.vertex(3), fixture.vertex(4));
    assert_eq!(fixture.vertex_at(a + near(2.0, 0.0)), Ok(Some(3)));
    assert_eq!(fixture.vertex_at(b - near(2.0, 0.0)), Ok(Some(4)));
    assert_eq!(fixture.vertex_at(fixture.vertex(5)), Ok(Some(5)));
}

#[test]
fn vertex_pick_skips_an_occluded_vertex_and_uses_its_own_tolerance() {
    let quad = |z: f32| {
        vec![
            world(-100.0, -100.0, 0.0),
            world(100.0, -100.0, 0.0),
            world(100.0, 100.0, 0.0),
            world(-100.0, 100.0, 0.0),
            world(0.0, 0.0, z),
        ]
    };
    for (z, expected) in [
        (-0.5, None),
        (0.5, Some(4)),
        (-0.0003, Some(4)),
        (-0.0015, Some(4)),
        (-0.02, Some(4)),
        (-0.05, None),
    ] {
        let fixture = Fixture::new(quad(z), &[vec![0, 1, 2, 3]], &[]);
        assert_eq!(fixture.vertex_at(fixture.vertex(4)), Ok(expected), "z {z}");
    }
}

#[test]
fn vertex_and_face_center_picks_read_their_own_depth_tolerance() {
    // A wide quad at z = 0 and, `gap` behind it and far from its centre, a triangle whose vertex
    // 4 and centre are both occluded by the quad's surface.
    for (gap, vertex, face) in [
        (0.00025, Some(4), Some(1)),
        (0.0015, Some(4), None),
        (0.05, None, None),
    ] {
        let points = vec![
            world(-280.0, -250.0, 0.0),
            world(280.0, -250.0, 0.0),
            world(280.0, 250.0, 0.0),
            world(-280.0, 250.0, 0.0),
            world(200.0, 0.0, -gap),
            world(240.0, 0.0, -gap),
            world(220.0, 40.0, -gap),
        ];
        let fixture = Fixture::new(points.clone(), &[vec![0, 1, 2, 3], vec![4, 5, 6]], &[]);
        assert_eq!(
            fixture.vertex_at(fixture.vertex(4)),
            Ok(vertex),
            "gap {gap}"
        );
        let center = pixel_of(
            (0..3)
                .map(|i| fixture.view.instances()[0].world_vertices()[4 + i])
                .sum::<Vec3>()
                / 3.0,
        );
        let picked = camera()
            .pick_face_center(
                &fixture.view,
                Some((fixture.owner, &fixture.path)),
                center,
                SIZE,
                1.0,
            )
            .map(|target| target.map(|target| target.face_index));
        assert_eq!(picked, Ok(face), "gap {gap}");
    }
}

/// Quad plate at z = 0 and a loose vertex `gap` behind it, seen from `distance`: the picked
/// vertex index at the vertex's own pixel.
fn pick_behind_plate(distance: f32, gap: f32) -> Result<Option<usize>, ViewError> {
    let camera = Camera::new(
        Vec3::ZERO,
        0.0,
        0.0,
        distance,
        45_f32.to_radians(),
        0.01,
        10_000.0,
    )
    .unwrap();
    let half = distance * 0.2;
    let points = [
        [-half, -half, 0.0],
        [half, -half, 0.0],
        [half, half, 0.0],
        [-half, half, 0.0],
        [0.0, 0.0, -gap],
    ]
    .map(|point: [f32; 3]| point.map(f64::from))
    .to_vec();
    let fixture = Fixture::new(points, &[vec![0, 1, 2, 3]], &[]);
    let ndc = camera.project(Vec3::new(0.0, 0.0, -gap), 800.0 / 600.0);
    let ndc = ndc.unwrap().unwrap();
    let pointer = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * Vec2::new(800.0, 600.0);
    camera
        .pick_vertex(
            &fixture.view,
            Some((fixture.owner, &fixture.path)),
            pointer,
            SIZE,
            1.0,
        )
        .map(|target| target.map(|target| target.vertex_index))
}

#[test]
fn a_thin_plate_in_front_of_a_vertex_hides_it_near_and_far() {
    // The slack is the flat tolerance * distance, so the gaps are set relative to it: half of it
    // the vertex counts as visible, twice it is hidden.
    for distance in [5.0_f32, 120.0, 500.0] {
        let slack = VERTEX_MARKER_DEPTH_TOLERANCE * distance;
        for (gap, expected) in [(slack * 0.5, Some(4)), (slack * 2.0, None)] {
            assert_eq!(
                pick_behind_plate(distance, gap),
                Ok(expected),
                "distance {distance} gap {gap}"
            );
        }
    }
}

#[test]
fn plate_thickness_at_the_default_distance_is_pinned_in_absolute_units() {
    // Independent of the tolerance constant: at distance 8, 0.1 units hides the vertex and 0.02
    // does not.
    assert_eq!(pick_behind_plate(8.0, 0.1), Ok(None));
    assert_eq!(pick_behind_plate(8.0, 0.02), Ok(Some(4)));
}

#[test]
fn component_picks_need_an_active_object_and_a_valid_viewport() {
    let points = vec![world(0.0, 0.0, 0.0), world(60.0, 0.0, 0.0)];
    let fixture = Fixture::new(points, &[], &[[0, 1]]);
    let center = fixture.vertex(0);
    let pick = |pointer, size: [u32; 2], ppp| {
        (
            camera().pick_vertex(&fixture.view, None, pointer, size, ppp),
            camera().pick_edge(&fixture.view, None, pointer, size, ppp),
        )
    };
    assert_eq!(pick(center, SIZE, 1.0), (Ok(None), Ok(None)));
    let other = [NativeId::from_decimal("424242").unwrap()];
    assert_eq!(
        camera().pick_vertex(
            &fixture.view,
            Some((fixture.owner, &other)),
            center,
            SIZE,
            1.0
        ),
        Ok(None)
    );
    let invalid = ViewError::Camera(InvalidCameraInput);
    for (pointer, size, ppp) in [
        (center, [0, 600], 1.0),
        (center, [800, 0], 1.0),
        (near(f32::NAN, 1.0), SIZE, 1.0),
        (center, SIZE, f32::NAN),
        (center, SIZE, 0.0),
    ] {
        assert_eq!(
            pick(pointer, size, ppp),
            (Err(invalid.clone()), Err(invalid.clone()))
        );
    }
}

#[test]
fn edge_pick_uses_screen_distance_and_reaches_face_less_edges() {
    let points = vec![
        world(-100.0, -100.0, 0.0),
        world(100.0, -100.0, 0.0),
        world(0.0, 100.0, 0.0),
        world(-300.0, 50.0, 0.0),
        world(-300.0, 150.0, 0.0),
    ];
    let fixture = Fixture::new(points, &[vec![0, 1, 2]], &[[3, 4]]);
    let bottom = (fixture.vertex(0) + fixture.vertex(1)) * 0.5;
    assert_eq!(fixture.edge_at(bottom), Ok(Some(0)));
    assert_eq!(fixture.edge_at(bottom + near(0.0, 5.9)), Ok(Some(0)));
    assert_eq!(fixture.edge_at(bottom + near(0.0, -5.9)), Ok(Some(0)));
    assert_eq!(fixture.edge_at(bottom + near(0.0, 6.2)), Ok(None));
    let loose = (fixture.vertex(3) + fixture.vertex(4)) * 0.5;
    assert_eq!(fixture.edge_at(loose), Ok(Some(3)));
}

#[test]
fn edge_pick_clips_at_the_near_plane_and_the_viewport_border() {
    let points = vec![
        [0.3, 0.0, 0.0],
        [0.3, 0.0, 20.0],
        world(0.0, -100.0, 0.0),
        world(5000.0, -100.0, 0.0),
        [0.0001, 0.0, 4.996],
        [0.0002, 0.0, 4.998],
    ];
    let fixture = Fixture::new(points, &[], &[[0, 1], [2, 3], [4, 5]]);
    // Between the eye and the near plane: ahead of the eye, but never drawn.
    assert_eq!(fixture.edge_at(near(418.0, 300.0)), Ok(None));
    let frame_for = |pointer| camera().pick_frame(pointer, SIZE, 1.0).unwrap();
    let on_edge = pixel_of(Vec3::new(0.3, 0.0, 3.0));
    let hit = edge_candidate(
        &frame_for(on_edge),
        Vec3::new(0.3, 0.0, 0.0),
        Vec3::new(0.3, 0.0, 20.0),
    )
    .unwrap()
    .unwrap();
    assert!((hit.point.z - 3.0).abs() < 1e-2, "{:?}", hit.point);
    assert_eq!(fixture.edge_at(on_edge), Ok(Some(0)));
    assert_eq!(
        fixture.edge_at(pixel_of(Vec3::new(0.3, 0.0, 0.0))),
        Ok(Some(0))
    );
    let visible = pixel_of(Vec3::new(unit(100.0), unit(-100.0), 0.0));
    assert_eq!(fixture.edge_at(visible), Ok(Some(1)));
}

#[test]
fn zero_length_edges_degrade_to_a_point_test() {
    let points = vec![world(0.0, 0.0, 0.0), world(100.0, 0.0, 0.0)];
    let fixture = Fixture::new(points, &[], &[[0, 0]]);
    let at = fixture.vertex(0);
    assert_eq!(fixture.edge_at(at), Ok(Some(0)));
    assert_eq!(fixture.edge_at(at + near(7.0, 0.0)), Ok(None));
    let p = Vec3::ZERO;
    let hit = edge_candidate(&camera().pick_frame(at, SIZE, 1.0).unwrap(), p, p)
        .unwrap()
        .unwrap();
    assert!(hit.pixels == 0.0 && hit.point.is_finite() && hit.ndc.is_finite());
}

#[test]
fn coincident_edges_resolve_to_the_lowest_edge_index() {
    let points = vec![
        world(-100.0, 0.0, 0.0),
        world(100.0, 0.0, 0.0),
        world(0.0, 100.0, 0.0),
        world(0.0, 200.0, 0.0),
        world(0.0, 300.0, 0.0),
    ];
    let loops = [vec![0, 1, 2], vec![0, 1, 3], vec![0, 1, 4]];
    let fixture = Fixture::new(points.clone(), &loops, &[]);
    let middle = (fixture.vertex(0) + fixture.vertex(1)) * 0.5;
    assert_eq!(fixture.edge_at(middle), Ok(Some(0)));
    let duplicated = Fixture::new(points, &[], &[[0, 1], [0, 1]]);
    assert_eq!(duplicated.edge_at(middle), Ok(Some(0)));
}

#[test]
fn edge_pick_returns_the_perspective_correct_point() {
    let (a, b) = (Vec3::new(0.3, 0.0, 4.0), Vec3::new(0.3, 0.0, -40.0));
    let pointer = (pixel_of(a) + pixel_of(b)) * 0.5;
    let hit = edge_candidate(&camera().pick_frame(pointer, SIZE, 1.0).unwrap(), a, b)
        .unwrap()
        .unwrap();
    assert!((pixel_of(hit.point) - pointer).length() < 0.05);
    assert!(hit.pixels < 0.05);
    assert!(
        hit.point.z > -10.0,
        "naive midpoint z is -18: {:?}",
        hit.point
    );
}

#[test]
fn edge_pick_skips_an_occluded_edge() {
    let edge_at = |z: f32| {
        vec![
            world(-100.0, -100.0, 0.0),
            world(100.0, -100.0, 0.0),
            world(100.0, 100.0, 0.0),
            world(-100.0, 100.0, 0.0),
            world(-50.0, 0.0, z),
            world(50.0, 0.0, z),
        ]
    };
    for (z, expected) in [(-0.5, None), (0.5, Some(4))] {
        let fixture = Fixture::new(edge_at(z), &[vec![0, 1, 2, 3]], &[[4, 5]]);
        assert_eq!(fixture.edge_at(near(400.0, 300.0)), Ok(expected), "z {z}");
    }
}

#[test]
fn pick_work_is_bounded_by_candidates_times_scene_triangles() {
    let build = |corners: usize| {
        let mut points: Vec<[f64; 3]> = (0..corners)
            .map(|i| {
                let angle = std::f64::consts::TAU * i as f64 / corners as f64;
                [50.0 * angle.cos(), 80.0 + 50.0 * angle.sin(), 0.0]
            })
            .collect();
        let shared = points.len();
        points.extend((0..1000).map(|_| world(0.0, -200.0, 0.0)));
        let loops = [(0..corners).collect()];
        let loose: Vec<[usize; 2]> = (0..1000).map(|_| [shared, shared]).collect();
        Fixture::new(points, &loops, &loose)
    };
    let admitted = build(1002);
    let click = pixel_of(Vec3::new(0.0, unit(-200.0), 0.0));
    assert_eq!(admitted.vertex_at(click), Ok(Some(1002)));
    assert_eq!(admitted.edge_at(click), Ok(Some(1002)));
    let refused = build(1003);
    assert_eq!(refused.vertex_at(click), Err(ViewError::ResourceLimit));
    assert_eq!(refused.edge_at(click), Err(ViewError::ResourceLimit));
}

#[test]
fn vertex_ranking_prefers_pixel_distance_over_depth() {
    let points = vec![world_at(20.0, 0.0, -1.0), world_at(23.0, 0.0, 1.0)];
    let fixture = Fixture::new(points, &[], &[]);
    let (far_but_close, near_but_far) = (fixture.vertex(0), fixture.vertex(1));
    assert!((far_but_close - near_but_far).abs().max_element() < 5.0);
    let pointer = far_but_close + near(0.5, 0.0);
    assert_eq!(fixture.vertex_at(pointer), Ok(Some(0)));
    assert_eq!(
        fixture.vertex_at(near_but_far - near(0.5, 0.0)),
        Ok(Some(1))
    );
}

#[test]
fn vertex_pick_rejects_points_just_outside_the_frustum() {
    let fixture = Fixture::new(vec![world_at(401.5, 0.0, 0.0), [0.0, 0.0, 4.995]], &[], &[]);
    // Right viewport border at pixel x = 800: a vertex 1.5 px outside is not on screen.
    assert_eq!(fixture.vertex_at(near(800.0, 300.0)), Ok(None));
    // On the view axis between the eye and the near plane.
    assert_eq!(fixture.vertex_at(near(400.0, 300.0)), Ok(None));
}

#[test]
fn vertex_and_edge_thresholds_include_their_exact_limit() {
    let fixture = Fixture::new(
        vec![
            world(0.0, 0.0, 0.0),
            world(-100.0, 0.0, 0.0),
            world(100.0, 0.0, 0.0),
        ],
        &[],
        &[[1, 2]],
    );
    assert_eq!(fixture.vertex(0), near(400.0, 300.0));
    assert_eq!(fixture.vertex_at(near(405.0, 300.0)), Ok(Some(0)));
    assert_eq!(fixture.vertex_at(near(400.0, 305.0)), Ok(Some(0)));
    assert_eq!(fixture.vertex_at(near(405.01, 300.0)), Ok(None));
    assert_eq!(fixture.edge_at(near(400.0, 306.0)), Ok(Some(0)));
    assert_eq!(fixture.edge_at(near(400.0, 306.01)), Ok(None));
}

#[test]
fn edge_crossing_the_viewport_border_is_picked_at_the_border() {
    for y in [-200.0, -100.0, -37.0, 61.0, 123.0] {
        let points = vec![world(-100.0, y, 0.0), world(5000.0, y, 0.0)];
        let fixture = Fixture::new(points, &[], &[[0, 1]]);
        let pointer = near(800.0, 300.0 - y);
        assert_eq!(fixture.edge_at(pointer), Ok(Some(0)), "y {y}");
    }
}

#[test]
fn equal_candidates_in_two_instances_resolve_to_the_lower_path() {
    let (native, owner) = test_fixtures::twin_shapes();
    let view = ViewScene::from_native(&native, 1 << 28).unwrap();
    let path = [native.node(owner).unwrap().transform_id()];
    assert_eq!(view.instances().len(), 2);
    let first = view.instances()[0].shape();
    assert_eq!(first, native.shapes()[0].id());
    let vertices = view.instances()[0].world_vertices();
    let front = (0..vertices.len())
        .rev()
        .max_by(|&a, &b| vertices[a].z.total_cmp(&vertices[b].z))
        .unwrap();
    let pointer = pixel_of(vertices[front]);
    let vertex = camera()
        .pick_vertex(&view, Some((owner, &path)), pointer, SIZE, 1.0)
        .unwrap()
        .unwrap();
    assert_eq!((vertex.shape, vertex.vertex_index), (first, front));
    let front_edge = |edge: &&[usize; 2]| edge.iter().all(|&end| vertices[end].z > 0.4);
    let first_edge = *view.instances()[0].edges().iter().find(front_edge).unwrap();
    let midpoint = (vertices[first_edge[0]] + vertices[first_edge[1]]) * 0.5;
    let edge = camera()
        .pick_edge(&view, Some((owner, &path)), pixel_of(midpoint), SIZE, 1.0)
        .unwrap()
        .unwrap();
    assert_eq!(edge.shape, first);
}

fn clip_point(axis: usize, value: f64) -> DVec4 {
    let mut point = DVec4::new(0.0, 0.0, 0.5, 1.0);
    point[axis] = value;
    point
}

#[test]
fn clip_segment_enforces_each_of_the_six_planes() {
    // (plane, axis, crossing span, expected interval, span fully outside)
    let planes = [
        ("near", 2, (-1.0, 1.0), (0.5, 1.0), (-2.0, -1.0)),
        ("far", 2, (0.0, 2.0), (0.0, 0.5), (2.0, 3.0)),
        ("left", 0, (-2.0, 0.0), (0.5, 1.0), (-3.0, -2.0)),
        ("right", 0, (0.0, 2.0), (0.0, 0.5), (2.0, 3.0)),
        ("bottom", 1, (-2.0, 0.0), (0.5, 1.0), (-3.0, -2.0)),
        ("top", 1, (0.0, 2.0), (0.0, 0.5), (2.0, 3.0)),
    ];
    for (name, axis, crossing, expected, outside) in planes {
        let clip =
            |(from, to): (f64, f64)| clip_segment(clip_point(axis, from), clip_point(axis, to));
        assert_eq!(clip(crossing), Some(expected), "{name} crossing");
        assert_eq!(clip(outside), None, "{name} outside");
        assert_eq!(clip((0.25, 0.75)), Some((0.0, 1.0)), "{name} inside");
    }
}

#[test]
fn clip_segment_keeps_a_segment_on_a_plane_and_rejects_a_parallel_one_outside() {
    // z == 0 for both ends: f0 == f1 == 0 on the near plane.
    let on_near = clip_segment(
        DVec4::new(0.0, 0.0, 0.0, 1.0),
        DVec4::new(0.5, 0.0, 0.0, 1.0),
    );
    assert_eq!(on_near, Some((0.0, 1.0)));
    // w - z == -0.5 for both ends: parallel to the far plane and outside it.
    let beyond_far = clip_segment(
        DVec4::new(0.0, 0.0, 2.0, 1.5),
        DVec4::new(0.5, 0.0, 2.0, 1.5),
    );
    assert_eq!(beyond_far, None);
}

#[test]
fn edge_pick_rejects_edges_clipped_by_the_border_and_the_far_plane() {
    let inside = Fixture::new(
        vec![world(397.0, -100.0, 0.0), world(397.0, 100.0, 0.0)],
        &[],
        &[[0, 1]],
    );
    // Three pixels inside the border: picked. Three outside: clipped, though within the click radius.
    assert_eq!(inside.edge_at(near(800.0, 300.0)), Ok(Some(0)));
    let outside_only = Fixture::new(
        vec![world(403.0, -100.0, 0.0), world(403.0, 100.0, 0.0)],
        &[],
        &[[0, 1]],
    );
    assert_eq!(outside_only.edge_at(near(800.0, 300.0)), Ok(None));
    // Beyond the far plane (eye at z = 5, far = 100) yet under the pointer.
    let beyond = Fixture::new(vec![[0.0, 0.0, -150.0], [0.5, 0.0, -150.0]], &[], &[[0, 1]]);
    assert_eq!(beyond.edge_at(near(400.0, 300.0)), Ok(None));
}

fn pose_camera() -> Camera {
    let mut camera = Camera::default();
    camera.orbit(Vec2::new(1.0, -0.3)).unwrap();
    camera.pan(Vec2::new(2.0, 1.0)).unwrap();
    camera
}

#[test]
fn set_pose_sets_the_pose_and_keeps_lens_and_clip() {
    let mut camera = pose_camera();
    let lens = Camera::default();
    camera
        .set_pose(Vec3::new(1.0, 2.0, 3.0), 4.5, 0.25, -0.5)
        .unwrap();
    assert_eq!(camera.target(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(camera.distance(), 4.5);
    assert_eq!(camera.yaw(), 0.25);
    assert_eq!(camera.pitch(), -0.5);
    assert_eq!(
        [camera.fov_y(), camera.near(), camera.far()],
        [lens.fov_y(), lens.near(), lens.far()]
    );
}

#[test]
fn set_pose_wraps_yaw_like_orbit() {
    let mut camera = Camera::default();
    camera.set_pose(Vec3::ZERO, 8.0, -0.5, 0.4).unwrap();
    assert_eq!(camera.yaw(), (-0.5_f32).rem_euclid(std::f32::consts::TAU));
    camera
        .set_pose(Vec3::ZERO, 8.0, 7.0 * std::f32::consts::TAU + 0.5, 0.4)
        .unwrap();
    assert!((0.0..std::f32::consts::TAU).contains(&camera.yaw()));
    assert!((camera.yaw() - 0.5).abs() < 1e-3);
}

#[test]
fn set_pose_round_trips_the_default_and_an_orbit_to_the_pole_exactly() {
    let mut camera = pose_camera();
    let default = Camera::default();
    camera
        .set_pose(
            default.target(),
            default.distance(),
            default.yaw(),
            default.pitch(),
        )
        .unwrap();
    assert!(camera.same_capture(&default));
    let mut polar = pose_camera();
    polar.orbit(Vec2::new(0.0, 10.0)).unwrap();
    let mut restored = Camera::default();
    restored
        .set_pose(polar.target(), polar.distance(), polar.yaw(), polar.pitch())
        .unwrap();
    assert!(restored.same_capture(&polar));
}

#[test]
fn yaw_stays_below_a_full_turn_and_its_pose_round_trips() {
    let tau = std::f32::consts::TAU;
    for steps in [&[-1e-9_f32][..], &[-0.6, -1e-9][..]] {
        let mut camera = Camera::default();
        camera.set_pose(Vec3::ZERO, 8.0, 0.0, 0.4).unwrap();
        for step in steps {
            camera.orbit(Vec2::new(*step, 0.0)).unwrap();
        }
        assert!((0.0..tau).contains(&camera.yaw()), "{steps:?}");
        let mut restored = camera.clone();
        let yaw = f64::from(camera.yaw()).to_degrees().to_radians() as f32;
        restored
            .set_pose(camera.target(), camera.distance(), yaw, camera.pitch())
            .unwrap();
        assert!(restored.same_capture(&camera), "{steps:?}");
    }
    let mut camera = Camera::default();
    camera.set_pose(Vec3::ZERO, 8.0, -1e-9, 0.4).unwrap();
    assert!((0.0..tau).contains(&camera.yaw()));
}

#[test]
fn set_pose_rejects_each_invalid_field_and_leaves_the_camera_unchanged() {
    use InvalidCameraPose as E;
    let limit = MAX_PITCH;
    let cases = [
        (
            Vec3::new(f32::NAN, 0.0, 0.0),
            8.0,
            0.0,
            0.0,
            E::TargetNotFinite,
        ),
        (
            Vec3::new(0.0, f32::INFINITY, 0.0),
            8.0,
            0.0,
            0.0,
            E::TargetNotFinite,
        ),
        (Vec3::ZERO, f32::NAN, 0.0, 0.0, E::DistanceNotFinite),
        (Vec3::ZERO, f32::INFINITY, 0.0, 0.0, E::DistanceNotFinite),
        (Vec3::ZERO, 0.0, 0.0, 0.0, E::DistanceNotPositive),
        (Vec3::ZERO, -1.0, 0.0, 0.0, E::DistanceNotPositive),
        (Vec3::ZERO, 8.0, f32::NAN, 0.0, E::YawNotFinite),
        (Vec3::ZERO, 8.0, f32::NEG_INFINITY, 0.0, E::YawNotFinite),
        (Vec3::ZERO, 8.0, 0.0, f32::NAN, E::PitchNotFinite),
        (Vec3::ZERO, 8.0, 0.0, f32::INFINITY, E::PitchNotFinite),
        (
            Vec3::ZERO,
            8.0,
            0.0,
            std::f32::consts::FRAC_PI_2,
            E::PitchOutOfRange,
        ),
        (Vec3::ZERO, 8.0, 0.0, limit.next_up(), E::PitchOutOfRange),
        (Vec3::ZERO, 8.0, 0.0, -limit.next_up(), E::PitchOutOfRange),
        (
            Vec3::splat(f32::MAX),
            f32::MAX,
            0.0,
            0.4,
            E::Unrepresentable,
        ),
        // Finite eye (equal to the target) but a NaN view matrix.
        (Vec3::splat(1e6), 1e-30, 0.0, 0.4, E::Unrepresentable),
    ];
    for (target, distance, yaw, pitch, expected) in cases {
        let mut camera = pose_camera();
        let before = camera.clone();
        assert_eq!(camera.set_pose(target, distance, yaw, pitch), Err(expected));
        assert!(
            camera.same_capture(&before),
            "{expected:?} changed the camera"
        );
    }
}

#[test]
fn set_pose_accepts_the_pitch_limit_and_names_the_bound_in_the_error() {
    let mut camera = Camera::default();
    camera.set_pose(Vec3::ZERO, 8.0, 0.0, MAX_PITCH).unwrap();
    camera.set_pose(Vec3::ZERO, 8.0, 0.0, -MAX_PITCH).unwrap();
    let message = InvalidCameraPose::PitchOutOfRange.to_string();
    assert!(
        message.contains("pitch") && message.contains("89.9427"),
        "{message}"
    );
}

fn new_at(yaw: f32, pitch: f32) -> Result<Camera, InvalidCameraInput> {
    Camera::new(
        Vec3::ZERO,
        yaw,
        pitch,
        8.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
}

#[test]
fn new_wraps_yaw_like_orbit_and_set_pose() {
    for yaw in [-0.5, std::f32::consts::TAU + 0.25, 7.0, -1e-9] {
        let built = new_at(yaw, 0.4).unwrap();
        assert!((0.0..std::f32::consts::TAU).contains(&built.yaw()), "{yaw}");
        let mut posed = Camera::default();
        posed.set_pose(Vec3::ZERO, 8.0, yaw, 0.4).unwrap();
        assert_eq!(built.yaw().to_bits(), posed.yaw().to_bits(), "{yaw}");
    }
    assert_eq!(new_at(f32::NAN, 0.0).err(), Some(InvalidCameraInput));
    assert_eq!(new_at(f32::INFINITY, 0.0).err(), Some(InvalidCameraInput));
}

#[test]
fn new_rejects_pitch_beyond_the_orbit_limit() {
    for pitch in [MAX_PITCH + 1e-4, -MAX_PITCH - 1e-4, 1.5705] {
        assert_eq!(
            new_at(0.0, pitch).err(),
            Some(InvalidCameraInput),
            "{pitch}"
        );
    }
    new_at(0.0, MAX_PITCH).unwrap();
    new_at(0.0, -MAX_PITCH).unwrap();
}

#[test]
fn new_keeps_an_in_range_yaw_and_negative_zero_bit_for_bit() {
    for yaw in [0.0_f32, -0.0, 0.6, 3.0, std::f32::consts::TAU - 1e-6] {
        assert_eq!(new_at(yaw, 0.0).unwrap().yaw().to_bits(), yaw.to_bits());
    }
}

#[test]
fn new_agrees_with_set_pose() {
    let yaws = [-7.0, -0.5, 0.0, 0.6, 7.0, f32::NAN];
    let pitches = [
        -1.6,
        -MAX_PITCH,
        0.0,
        0.4,
        MAX_PITCH,
        MAX_PITCH + 1e-4,
        1.5705,
    ];
    for yaw in yaws {
        for pitch in pitches {
            let mut posed = Camera::default();
            let set = posed.set_pose(Vec3::ZERO, 8.0, yaw, pitch);
            match new_at(yaw, pitch) {
                Ok(built) => {
                    assert!(set.is_ok(), "{yaw} {pitch}");
                    assert_eq!(built.yaw().to_bits(), posed.yaw().to_bits());
                    assert_eq!(built.pitch().to_bits(), posed.pitch().to_bits());
                }
                Err(_) => assert!(set.is_err(), "{yaw} {pitch}"),
            }
        }
    }
}

/// Changed/Unchanged is numeric, so a render-identical -0.0 pose is no change even though
/// `same_capture`, which cancels a live drag, compares bits.
#[test]
fn set_pose_with_negative_zero_is_numerically_equal_but_not_the_same_capture() {
    let zero = new_at(0.0, 0.0).unwrap();
    let mut flipped = zero.clone();
    flipped
        .set_pose(Vec3::new(-0.0, -0.0, -0.0), 8.0, -0.0, -0.0)
        .unwrap();
    assert_eq!(flipped, zero);
    assert_eq!(zero, flipped);
    assert!(!flipped.same_capture(&zero));
    assert!(!zero.same_capture(&flipped));
    assert_eq!(flipped.yaw().to_bits(), (-0.0_f32).to_bits());
    assert_eq!(flipped.pitch().to_bits(), (-0.0_f32).to_bits());
}

/// The default view is a documented-by-us constant: any drift must be a conscious change.
#[test]
fn default_camera_is_pinned_bit_for_bit() {
    let camera = Camera::default();
    assert_eq!(
        camera.target().to_array().map(f32::to_bits),
        [0.0_f32; 3].map(f32::to_bits)
    );
    let scalars = [
        camera.distance(),
        camera.yaw(),
        camera.pitch(),
        camera.fov_y(),
        camera.near(),
        camera.far(),
    ];
    let expected = [8.0, 0.6, 0.4, 45_f32.to_radians(), 0.01, 10_000.0];
    assert_eq!(scalars.map(f32::to_bits), expected.map(f32::to_bits));
}
