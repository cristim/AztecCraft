//! Vertex markers and Edge-mode drawing: submission payloads, then pixels through the real
//! pipelines on a headless GPU. Pixel checks prove depth behaviour of the exercised cameras, not
//! Maya parity; colours and sizes are provisional (see `known-issues.md`).

use super::*;
use azteccraft::{
    native_scene::{EdgeTarget, VertexTarget},
    viewport::{FACE_CENTER_DEPTH_TOLERANCE, VERTEX_MARKER_DEPTH_TOLERANCE},
};

const MARKER_PIXELS: f32 = 5.0;

struct Scene {
    native: NativeScene,
    view: ViewScene,
    owner: NodeId,
    path: Vec<NativeId>,
}

impl Scene {
    fn new(native: NativeScene) -> Self {
        let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
        let owner = native.nodes()[0].id();
        let instance = view
            .instances()
            .iter()
            .find(|instance| instance.transform() == owner)
            .unwrap();
        let path = instance.path()[..instance.path().len() - 1].to_vec();
        Self {
            native,
            view,
            owner,
            path,
        }
    }

    fn active(&self) -> Option<(NodeId, &[NativeId])> {
        Some((self.owner, &self.path))
    }

    fn instance(&self) -> &ViewInstance {
        self.view
            .instances()
            .iter()
            .find(|instance| instance.transform() == self.owner)
            .unwrap()
    }

    fn vertex_target(&self, vertex_index: usize) -> VertexTarget {
        let instance = self.instance();
        let target = VertexTarget {
            transform: self.owner,
            shape: instance.shape(),
            path: instance.path().to_vec(),
            vertex_index,
        };
        self.native.resolve_vertex_target(&target).unwrap();
        target
    }

    fn edge_target(&self, edge_index: usize) -> EdgeTarget {
        let instance = self.instance();
        let target = EdgeTarget {
            transform: self.owner,
            shape: instance.shape(),
            path: instance.path().to_vec(),
            edge_index,
        };
        self.native.resolve_edge_target(&target).unwrap();
        target
    }
}

fn submit(
    scene: &Scene,
    camera: &Camera,
    selection: SelectionFeedback<'_>,
    shading: ShadingMode,
) -> Submission {
    Submission::new(
        &scene.view,
        camera,
        [SIZE; 2],
        selection,
        true,
        shading,
        FORMAT,
        u64::MAX,
    )
    .unwrap()
}

fn vertex_feedback<'a>(
    scene: &'a Scene,
    selected: Option<&'a VertexTarget>,
) -> SelectionFeedback<'a> {
    SelectionFeedback::Vertex {
        active: scene.active(),
        selected,
    }
}

fn edge_feedback<'a>(scene: &'a Scene, selected: Option<&'a EdgeTarget>) -> SelectionFeedback<'a> {
    SelectionFeedback::Edge {
        active: scene.active(),
        selected,
    }
}

fn rgba(rgb: [u8; 3]) -> [f32; 4] {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2]).to_normalized_gamma_f32()
}

fn vertices(submission: &Submission, range: &Range<u32>) -> Vec<[f32; 8]> {
    range
        .clone()
        .map(|vertex| {
            let start = vertex as usize * VERTEX_BYTES;
            std::array::from_fn(|at| {
                f32::from_ne_bytes(
                    submission.bytes[start + at * 4..start + at * 4 + 4]
                        .try_into()
                        .unwrap(),
                )
            })
        })
        .collect()
}

fn color(vertex: &[f32; 8]) -> [f32; 4] {
    [vertex[4], vertex[5], vertex[6], vertex[7]]
}

fn pixels_of(vertex: &[f32; 8]) -> Vec2 {
    Vec2::new(vertex[0], vertex[1]) / vertex[3] * (SIZE as f32 * 0.5)
}

#[test]
fn vertex_mode_marks_every_vertex_with_the_selected_one_last_in_its_own_colour() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let object = submit(
        &scene,
        &camera,
        SelectionFeedback::Object(None),
        ShadingMode::SmoothShadeAll,
    );
    for selected in [None, Some(scene.vertex_target(3))] {
        let submission = submit(
            &scene,
            &camera,
            vertex_feedback(&scene, selected.as_ref()),
            ShadingMode::SmoothShadeAll,
        );
        // Outlines and surfaces are the Object-mode ones; only the marker range is new.
        assert_eq!(submission.surfaces, object.surfaces);
        assert_eq!(submission.lines, object.lines);
        let markers = vertices(&submission, &submission.markers);
        assert_eq!(markers.len(), 8 * 6);
        let colors: Vec<_> = markers.chunks(6).map(|quad| color(&quad[0])).collect();
        let unselected = rgba(VERTEX_RGB);
        match selected {
            None => assert!(colors.iter().all(|color| *color == unselected)),
            Some(_) => {
                assert_eq!(colors[7], rgba(SELECTED_VERTEX_RGB));
                assert!(colors[..7].iter().all(|color| *color == unselected));
                let quad = &markers[42..];
                let center = quad.iter().map(pixels_of).sum::<Vec2>() / 6.0;
                let expected = camera
                    .project(
                        camera.marker_biased(scene.instance().world_vertices()[3]),
                        1.0,
                    )
                    .unwrap()
                    .unwrap();
                assert!((center - expected.truncate() * (SIZE as f32 * 0.5)).length() < 1e-3);
            }
        }
        for quad in markers.chunks(6) {
            let points: Vec<_> = quad.iter().map(pixels_of).collect();
            let min = points.iter().copied().fold(Vec2::INFINITY, Vec2::min);
            let max = points.iter().copied().fold(Vec2::NEG_INFINITY, Vec2::max);
            assert!((max - min - Vec2::splat(MARKER_PIXELS)).abs().max_element() < 1e-3);
        }
    }
}

#[test]
fn vertex_mode_without_an_active_object_or_in_other_modes_draws_no_markers() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let target = scene.vertex_target(0);
    for selection in [
        SelectionFeedback::Vertex {
            active: None,
            selected: Some(&target),
        },
        SelectionFeedback::Object(Some(&scene.path)),
        edge_feedback(&scene, None),
    ] {
        let submission = submit(&scene, &camera, selection, ShadingMode::SmoothShadeAll);
        assert!(submission.markers.is_empty());
    }
}

#[test]
fn vertex_markers_off_screen_are_skipped_not_clamped() {
    let scene = Scene::new(cube());
    let camera = Camera::new(
        Vec3::new(2.0, 0.0, 0.0),
        0.0,
        0.0,
        5.0,
        45_f32.to_radians(),
        0.01,
        100.0,
    )
    .unwrap();
    let visible = scene
        .instance()
        .world_vertices()
        .iter()
        .filter(|point| {
            camera
                .project(**point, 1.0)
                .unwrap()
                .is_some_and(|ndc| ndc.x.abs() <= 1.0 && ndc.y.abs() <= 1.0)
        })
        .count();
    assert_eq!(visible, 4);
    let submission = submit(
        &scene,
        &camera,
        vertex_feedback(&scene, None),
        ShadingMode::SmoothShadeAll,
    );
    assert_eq!(submission.markers.len(), visible * 6);
}

#[test]
fn vertex_marker_bytes_obey_the_device_limit_boundary_and_report_resource_limit() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let selected = scene.vertex_target(0);
    let selection = vertex_feedback(&scene, Some(&selected));
    let bytes = submit(&scene, &camera, selection, ShadingMode::SmoothShadeAll)
        .bytes
        .len() as u64;
    let with_limit = |limit| {
        Submission::new(
            &scene.view,
            &camera,
            [SIZE; 2],
            selection,
            true,
            ShadingMode::SmoothShadeAll,
            FORMAT,
            limit,
        )
        .map(|_| ())
    };
    assert_eq!(with_limit(bytes), Ok(()));
    assert_eq!(with_limit(bytes - 1), Err(RenderError::ResourceLimit));
}

#[test]
fn wireframe_vertex_markers_hide_behind_surfaces_the_picker_also_rejects() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let wire = submit(
        &scene,
        &camera,
        vertex_feedback(&scene, None),
        ShadingMode::Wireframe,
    );
    // Four front corners are unoccluded from the face-on camera; the back four sit behind the
    // front face and are filtered like face centres.
    assert_eq!(wire.markers.len(), 4 * 6);
    for (index, point) in scene.instance().world_vertices().iter().enumerate() {
        let ndc = camera.project(*point, 1.0).unwrap().unwrap();
        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * SIZE as f32;
        let picked = camera
            .pick_vertex(&scene.view, scene.active(), pixel, [SIZE; 2], 1.0)
            .unwrap()
            .map(|target| target.vertex_index);
        assert_eq!(picked == Some(index), point.z > 0.0, "vertex {index}");
    }
}

#[test]
fn edge_mode_draws_every_edge_instead_of_outlines_for_the_active_instance_only() {
    let mut native = cube();
    let second = native.create(Primitive::Cube, "other").unwrap();
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [3.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let scene = Scene::new(native);
    let camera = camera(0.01);
    let object = submit(
        &scene,
        &camera,
        SelectionFeedback::Object(None),
        ShadingMode::SmoothShadeAll,
    );
    let submission = submit(
        &scene,
        &camera,
        edge_feedback(&scene, None),
        ShadingMode::SmoothShadeAll,
    );
    assert_eq!(submission.surfaces, object.surfaces);
    assert!(submission.markers.is_empty());
    // 12 edges replace the active cube's 6 quads x 4 outline segments; the other cube keeps its 48.
    let edges = scene.instance().edges().len();
    assert_eq!(edges, 12);
    let lines = vertices(&submission, &submission.lines);
    assert_eq!(lines.len(), GRID_VERTICES + edges * 2 + 48);
    let edge_color = rgba(EDGE_RGB);
    let blue = lines.iter().filter(|vertex| color(vertex) == edge_color);
    assert_eq!(blue.count(), edges * 2);
    assert_eq!(
        lines
            .iter()
            .filter(|vertex| color(vertex) == rgba([145; 3]))
            .count(),
        48
    );
}

#[test]
fn edge_mode_draws_each_listed_edge_between_its_own_endpoints() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let submission = submit(
        &scene,
        &camera,
        edge_feedback(&scene, None),
        ShadingMode::SmoothShadeAll,
    );
    let matrix = camera.projection_matrix(1.0).unwrap() * camera.view_matrix();
    let lines = vertices(&submission, &submission.lines);
    let drawn = &lines[GRID_VERTICES..];
    for (&[a, b], pair) in scene.instance().edges().iter().zip(drawn.chunks(2)) {
        for (vertex, end) in pair.iter().zip([a, b]) {
            let clip = matrix * scene.instance().world_vertices()[end].extend(1.0);
            assert_eq!(vertex[..4], clip.to_array());
        }
    }
}

#[test]
fn selected_edge_is_a_three_pixel_strip_on_top_in_its_own_colour_and_stale_indices_draw_nothing() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let selected = scene.edge_target(0);
    let submission = submit(
        &scene,
        &camera,
        edge_feedback(&scene, Some(&selected)),
        ShadingMode::SmoothShadeAll,
    );
    let quad = vertices(&submission, &submission.markers);
    assert_eq!(quad.len(), 6);
    assert!(
        quad.iter()
            .all(|vertex| color(vertex) == rgba(SELECTED_EDGE_RGB))
    );
    // Both ends are offset perpendicular to the edge by half the width on each side.
    let [a, b] = scene.instance().edges()[0];
    let ends = [a, b].map(|index| {
        let point = camera.marker_biased(scene.instance().world_vertices()[index]);
        let ndc = camera.project(point, 1.0).unwrap().unwrap();
        ndc.truncate() * (SIZE as f32 * 0.5)
    });
    let along = (ends[1] - ends[0]).normalize();
    let across = Vec2::new(-along.y, along.x);
    for vertex in &quad {
        let pixel = pixels_of(vertex);
        let end = if (pixel - ends[0]).dot(along).abs() < (pixel - ends[1]).dot(along).abs() {
            ends[0]
        } else {
            ends[1]
        };
        assert!((pixel - end).dot(along).abs() < 1e-3);
        assert!(((pixel - end).dot(across).abs() - 1.5).abs() < 1e-3);
    }
    let stale = EdgeTarget {
        edge_index: 12,
        ..selected
    };
    let none = submit(
        &scene,
        &camera,
        edge_feedback(&scene, Some(&stale)),
        ShadingMode::SmoothShadeAll,
    );
    assert!(none.markers.is_empty());
}

#[test]
fn selected_edge_is_cut_at_the_near_plane_and_dropped_behind_the_eye() {
    let scene = Scene::new(cube());
    let along_z = scene
        .instance()
        .edges()
        .iter()
        .position(|&[a, b]| {
            let points = scene.instance().world_vertices();
            points[a].x == points[b].x && points[a].y == points[b].y
        })
        .unwrap();
    let selected = scene.edge_target(along_z);
    let from = |target_z: f32, distance: f32| {
        Camera::new(
            Vec3::new(0.0, 0.0, target_z),
            0.0,
            0.0,
            distance,
            45_f32.to_radians(),
            0.01,
            100.0,
        )
        .unwrap()
    };
    // Eye at z=0.5 looks down -Z: the edge's z=1 end is behind it.
    let crossing = submit(
        &scene,
        &from(-10.0, 10.5),
        edge_feedback(&scene, Some(&selected)),
        ShadingMode::SmoothShadeAll,
    );
    let quad = vertices(&crossing, &crossing.markers);
    assert_eq!(quad.len(), 6);
    assert!(
        quad.iter()
            .all(|vertex| vertex[3] > 0.0 && vertex[2] >= 0.0)
    );
    // Eye at z=-5 looks down -Z: the whole cube is behind it.
    let behind = submit(
        &scene,
        &from(-10.0, 5.0),
        edge_feedback(&scene, Some(&selected)),
        ShadingMode::SmoothShadeAll,
    );
    assert!(behind.markers.is_empty());
}

#[test]
fn depth_tolerances_are_pinned_and_the_marker_bias_reads_the_vertex_constant() {
    assert_eq!(FACE_CENTER_DEPTH_TOLERANCE, 1e-4);
    assert_eq!(VERTEX_MARKER_DEPTH_TOLERANCE, 6e-3);
    let camera = Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 1.0, 1.0, 100.0).unwrap();
    let moved = camera.marker_biased(Vec3::ZERO);
    assert!((moved.z - 5.0 * VERTEX_MARKER_DEPTH_TOLERANCE).abs() < 1e-6);
}

/// A cube of side 2 so its corners are well apart on screen.
fn big_cube() -> Scene {
    let mut native = cube();
    let owner = native.nodes()[0].id();
    native
        .set_transform(
            owner,
            NativeTransform {
                scale: [2.0; 3],
                ..Default::default()
            },
        )
        .unwrap();
    Scene::new(native)
}

fn marker_pixels(camera: &Camera, point: Vec3, size: u32) -> Vec<(u32, u32)> {
    let ndc = camera.project(point, 1.0).unwrap().unwrap();
    let center = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * size as f32;
    let half = MARKER_PIXELS * 0.5;
    let mut inside = Vec::new();
    for y in 0..size {
        for x in 0..size {
            let offset = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) - center;
            if offset.abs().max_element() < half {
                inside.push((x, y));
            }
        }
    }
    inside
}

fn sized_pixel(image: &[u8], size: u32, x: u32, y: u32) -> &[u8] {
    let offset = ((y * size + x) * 4) as usize;
    &image[offset..offset + 4]
}

/// Corners with a front-facing face: the ones a marker should show on.
fn visible_corners(scene: &Scene, camera: &Camera) -> Vec<bool> {
    let instance = scene.instance();
    let eye = camera.eye();
    let mut visible = vec![false; instance.world_vertices().len()];
    for face in instance.faces() {
        for &corner in face.corners() {
            if instance
                .world_normal(face)
                .dot(eye - instance.world_vertices()[corner])
                > 0.0
            {
                visible[corner] = true;
            }
        }
    }
    visible
}

/// Renders Vertex mode at `size` px and returns one line per corner whose marker is not whole
/// (visible corners) or not hidden (the others); empty when every corner is right.
fn corner_failures(camera: &Camera, size: u32) -> Vec<String> {
    let scene = big_cube();
    let mut gpu = Offscreen::new();
    gpu.size = size;
    let draw = |gpu: &mut Offscreen, selection| {
        let submission = Submission::new(
            &scene.view,
            camera,
            [size; 2],
            selection,
            true,
            ShadingMode::SmoothShadeAll,
            FORMAT,
            u64::MAX,
        )
        .unwrap();
        gpu.render(TestFrame::Geometry(submission))
    };
    let plain = draw(&mut gpu, SelectionFeedback::Object(None));
    let marked = draw(&mut gpu, vertex_feedback(&scene, None));
    let marker = rgba(VERTEX_RGB).map(|channel| (channel * 255.0).round() as u8);
    let visible = visible_corners(&scene, camera);
    let points = scene.instance().world_vertices();
    let shown: Vec<_> = (0..points.len())
        .filter(|&i| visible[i])
        .flat_map(|i| marker_pixels(camera, points[i], size))
        .collect();
    let mut failures = Vec::new();
    for (index, &point) in points.iter().enumerate() {
        let area = marker_pixels(camera, point, size);
        assert_eq!(area.len(), 25);
        if visible[index] {
            let whole = area
                .iter()
                .filter(|&&(x, y)| sized_pixel(&marked, size, x, y) == marker)
                .count();
            if whole != 25 {
                failures.push(format!("visible corner {index}: {whole}/25 marker pixels"));
            }
        } else {
            // Pixels another visible marker covers legitimately change.
            let changed = area
                .iter()
                .filter(|pixel| !shown.contains(pixel))
                .filter(|&&(x, y)| {
                    sized_pixel(&marked, size, x, y) != sized_pixel(&plain, size, x, y)
                })
                .count();
            if changed != 0 {
                failures.push(format!("hidden corner {index}: {changed} pixels drawn"));
            }
        }
    }
    failures
}

// Readbacks ran on an Apple M1 Pro (Metal) with a side-2 cube at the origin, near 0.01 and far
// 10000, yaw 0.6 and pitch 0.4 for the far cameras; see `known-issues.md` for the table.
#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_every_visible_corner_marker_is_whole_at_the_default_distance_and_1024_px() {
    assert_eq!(
        corner_failures(&Camera::default(), 1024),
        Vec::<String>::new()
    );
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_every_visible_corner_marker_is_whole_at_a_far_distance_and_1024_px() {
    let camera = Camera::new(
        Vec3::ZERO,
        0.6,
        0.4,
        120.0,
        45_f32.to_radians(),
        0.01,
        10_000.0,
    )
    .unwrap();
    assert_eq!(corner_failures(&camera, 1024), Vec::<String>::new());
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_far_small_object_corner_markers_are_whole_at_128_px() {
    for (distance, fov_degrees) in [(120.0, 2.0), (300.0, 1.0)] {
        let camera = Camera::new(
            Vec3::ZERO,
            0.6,
            0.4,
            distance,
            f32::to_radians(fov_degrees),
            0.01,
            10_000.0,
        )
        .unwrap();
        assert_eq!(
            corner_failures(&camera, SIZE),
            Vec::<String>::new(),
            "d={distance}"
        );
    }
}

#[test]
#[ignore = "requires a real headless GPU adapter"]
fn headless_gpu_edge_mode_draws_all_edges_without_outlines_and_the_selected_strip_on_top() {
    let scene = big_cube();
    let camera = camera(0.01);
    let mut gpu = Offscreen::new();
    let edge = rgba(EDGE_RGB).map(|channel| (channel * 255.0).round() as u8);
    // Wireframe has no surface depth, so all 12 edges must show. Outline suppression is a
    // payload property: edges coincide with outlines, so pixels cannot tell them apart.
    let wire = gpu.render(TestFrame::Geometry(submit(
        &scene,
        &camera,
        edge_feedback(&scene, None),
        ShadingMode::Wireframe,
    )));
    let points = scene.instance().world_vertices();
    for (index, &[a, b]) in scene.instance().edges().iter().enumerate() {
        let middle = (points[a] + points[b]) * 0.5;
        let ndc = camera.project(middle, 1.0).unwrap().unwrap();
        let (x, y) = (
            ((ndc.x + 1.0) * 0.5 * SIZE as f32) as i32,
            ((1.0 - ndc.y) * 0.5 * SIZE as f32) as i32,
        );
        assert!(
            (-1..=1)
                .any(|dy| (-1..=1)
                    .any(|dx| { pixel(&wire, (x + dx) as u32, (y + dy) as u32) == edge })),
            "edge {index} at ({x}, {y})"
        );
    }
    // The front face's top edge, selected, is a strip over the surface and over its own line.
    let top = scene
        .instance()
        .edges()
        .iter()
        .position(|&[a, b]| {
            points[a].y == 1.0 && points[b].y == 1.0 && points[a].z == 1.0 && points[b].z == 1.0
        })
        .unwrap();
    let selected = scene.edge_target(top);
    let shaded = gpu.render(TestFrame::Geometry(submit(
        &scene,
        &camera,
        edge_feedback(&scene, Some(&selected)),
        ShadingMode::SmoothShadeAll,
    )));
    let [a, b] = scene.instance().edges()[top];
    let ndc = camera
        .project((points[a] + points[b]) * 0.5, 1.0)
        .unwrap()
        .unwrap();
    let (x, y) = (
        ((ndc.x + 1.0) * 0.5 * SIZE as f32) as u32,
        ((1.0 - ndc.y) * 0.5 * SIZE as f32) as u32,
    );
    let strip = rgba(SELECTED_EDGE_RGB).map(|channel| (channel * 255.0).round() as u8);
    assert_eq!(pixel(&shaded, x, y), strip);
    assert_eq!(pixel(&shaded, x, y - 1), strip);
    assert_ne!(pixel(&shaded, x, y - 3), strip);
}

#[test]
fn a_remembered_edge_on_a_non_active_instance_draws_no_strip() {
    let mut native = cube();
    let second = native.create(Primitive::Cube, "other").unwrap();
    native
        .set_transform(
            second,
            NativeTransform {
                translation: [3.0, 0.0, 0.0],
                ..Default::default()
            },
        )
        .unwrap();
    let scene = Scene::new(native);
    let other = scene
        .view
        .instances()
        .iter()
        .find(|instance| instance.transform() == second)
        .unwrap();
    let target = EdgeTarget {
        transform: second,
        shape: other.shape(),
        path: other.path().to_vec(),
        edge_index: 0,
    };
    scene.native.resolve_edge_target(&target).unwrap();
    let camera = camera(0.01);
    let submission = submit(
        &scene,
        &camera,
        edge_feedback(&scene, Some(&target)),
        ShadingMode::SmoothShadeAll,
    );
    assert!(submission.markers.is_empty());
}

#[test]
fn wireframe_vertex_filter_uses_the_vertex_tolerance_not_the_face_one() {
    // A unit cube whose front face sits `gap` behind the front face of a side-2 plate cube: 2.5e-3
    // of the 4-unit distance, between the face tolerance (1e-4) and the vertex tolerance (6e-3).
    let gap = 0.01;
    let mut native = cube();
    let plate = native.create(Primitive::Cube, "plate").unwrap();
    native
        .set_transform(
            plate,
            NativeTransform {
                scale: [2.0; 3],
                ..Default::default()
            },
        )
        .unwrap();
    let first = native.nodes()[0].id();
    native
        .set_transform(
            first,
            NativeTransform {
                translation: [0.0, 0.0, 0.5 - gap],
                ..Default::default()
            },
        )
        .unwrap();
    let scene = Scene::new(native);
    let camera = camera(0.01);
    let wire = submit(
        &scene,
        &camera,
        vertex_feedback(&scene, None),
        ShadingMode::Wireframe,
    );
    // Only the four front corners are within the slack; the back four sit a unit deeper.
    assert_eq!(wire.markers.len(), 4 * 6);
}

#[test]
fn the_selected_edge_strip_sits_toward_the_eye_of_the_edge_itself() {
    let scene = Scene::new(cube());
    let camera = camera(0.01);
    let selected = scene.edge_target(0);
    let submission = submit(
        &scene,
        &camera,
        edge_feedback(&scene, Some(&selected)),
        ShadingMode::SmoothShadeAll,
    );
    let quad = vertices(&submission, &submission.markers);
    let [a, b] = scene.instance().edges()[0];
    let points = scene.instance().world_vertices();
    for end in [a, b] {
        let on_edge = camera.project(points[end], 1.0).unwrap().unwrap();
        let biased = camera
            .project(camera.marker_biased(points[end]), 1.0)
            .unwrap()
            .unwrap();
        assert!(biased.z < on_edge.z - 1e-6);
        let nearest = quad
            .iter()
            .min_by(|p, q| {
                let distance =
                    |v: &&[f32; 8]| (Vec2::new(v[0], v[1]) / v[3] - biased.truncate()).length();
                distance(p).total_cmp(&distance(q))
            })
            .unwrap();
        assert!((nearest[2] / nearest[3] - biased.z).abs() < 1e-6);
    }
}

/// A cube with `extrusions` extrusions of face 1 plus `planes` two-triangle planes. At 123
/// extrusions the cube has 500 vertices and 996 triangles, so 502 planes make the work exactly
/// 500 * 2000 = 1_000_000, the occlusion budget.
fn budget_scene(extrusions: usize, planes: usize) -> Scene {
    let mut native = cube();
    let shape = native.shapes()[0].id();
    for _ in 0..extrusions {
        native.extrude_face(shape, 1, 0.01).unwrap();
    }
    for index in 0..planes {
        native
            .create_with_options(
                Primitive::Plane,
                &SINGLE_QUAD_PLANE,
                &format!("plane{index}"),
            )
            .unwrap();
    }
    Scene::new(native)
}

#[test]
fn wireframe_vertex_markers_hide_exactly_when_vertices_times_triangles_exceed_the_budget() {
    let camera = camera(0.01);
    for (planes, hidden) in [(502, false), (503, true)] {
        let scene = budget_scene(123, planes);
        let triangles: usize = scene
            .view
            .instances()
            .iter()
            .flat_map(|instance| instance.faces())
            .map(|face| face.triangles().count())
            .sum();
        let work = scene.instance().world_vertices().len() * triangles;
        assert_eq!(work == 1_000_000, !hidden, "{planes}: {work}");
        assert_eq!(work > 1_000_000, hidden, "{planes}: {work}");
        let selection = vertex_feedback(&scene, None);
        let wire = submit(&scene, &camera, selection, ShadingMode::Wireframe);
        assert_eq!(wire.centers_degraded, hidden, "{planes}");
        assert_eq!(wire.markers.is_empty(), hidden, "{planes}");
        let flagged = renderer_handle()
            .callback(
                &scene.view,
                &camera,
                Rect::from_min_size(egui::Pos2::ZERO, egui::Vec2::splat(128.0)),
                [SIZE; 2],
                selection,
                true,
                ShadingMode::Wireframe,
            )
            .unwrap()
            .1;
        assert_eq!(flagged, hidden, "{planes}");
    }
}

#[allow(dead_code)]
mod binary_fixture {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/maya_binary.rs"
    ));
}

/// A one-triangle mesh two transforms below a root whose X scale is -1.
fn mirrored_parent_scene() -> Scene {
    use binary_fixture::{chunk, creation, group, mesh_payload, scene, triple};
    let bytes = scene(
        &[
            group(
                b"XFRM",
                &[
                    creation(b"mirror", None),
                    triple(b"t", [0.5, 0.0, 0.0]),
                    triple(b"s", [-1.0, 1.0, 1.0]),
                ]
                .concat(),
                4,
            ),
            group(b"XFRM", &creation(b"object", Some(b"mirror")), 4),
            group(
                b"DMSH",
                &[
                    creation(b"shape", Some(b"object")),
                    chunk(b"MESH", &mesh_payload(&[0, 1, 0x6000_0002]), 8),
                ]
                .concat(),
                4,
            ),
        ]
        .concat(),
    );
    let native = azteccraft::native_scene::binary::decode(bytes).unwrap();
    let view = ViewScene::from_native(&native, VIEW_BUDGET).unwrap();
    let instance = &view.instances()[0];
    let owner = instance.transform();
    let path = instance.path()[..instance.path().len() - 1].to_vec();
    assert_eq!(path.len(), 2, "the shape must sit below a parent");
    Scene {
        native,
        view,
        owner,
        path,
    }
}

#[test]
fn vertex_markers_and_picks_follow_world_vertices_under_a_mirrored_parent() {
    let scene = mirrored_parent_scene();
    let camera = camera(0.01);
    let points = scene.instance().world_vertices().to_vec();
    assert_eq!(points.len(), 3);
    // Local (1, 0, 0) lands at x = 0.5 - 1 = -0.5: only the mirror puts it left of the origin.
    assert!(points[1].x < points[0].x, "{points:?}");
    let submission = submit(
        &scene,
        &camera,
        vertex_feedback(&scene, None),
        ShadingMode::SmoothShadeAll,
    );
    let markers = vertices(&submission, &submission.markers);
    assert_eq!(markers.len(), 3 * 6);
    for (index, point) in points.iter().enumerate() {
        let expected = camera
            .project(camera.marker_biased(*point), 1.0)
            .unwrap()
            .unwrap();
        let center = markers[index * 6..][..6]
            .iter()
            .map(pixels_of)
            .sum::<Vec2>()
            / 6.0;
        assert!(
            (center - expected.truncate() * (SIZE as f32 * 0.5)).length() < 1e-3,
            "marker {index}"
        );
        let ndc = camera.project(*point, 1.0).unwrap().unwrap();
        let pixel = Vec2::new(ndc.x + 1.0, 1.0 - ndc.y) * 0.5 * SIZE as f32;
        let picked = camera
            .pick_vertex(&scene.view, scene.active(), pixel, [SIZE; 2], 1.0)
            .unwrap();
        assert_eq!(picked.map(|target| target.vertex_index), Some(index));
    }
}
