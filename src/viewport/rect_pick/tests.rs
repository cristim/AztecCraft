use super::*;

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
    assert_eq!(clip_triangle_depth(tri), tri);
}

#[test]
fn clip_drops_a_triangle_behind_the_near_plane_or_beyond_the_far_plane() {
    let behind = [
        clip(0.0, 0.0, -0.5, 1.0),
        clip(1.0, 0.0, -0.1, 1.0),
        clip(0.0, 1.0, -0.2, 1.0),
    ];
    assert!(clip_triangle_depth(behind).is_empty());
    let beyond = [
        clip(0.0, 0.0, 2.0, 1.0),
        clip(1.0, 0.0, 1.5, 1.0),
        clip(0.0, 1.0, 3.0, 1.0),
    ];
    assert!(clip_triangle_depth(beyond).is_empty());
}

#[test]
fn clip_cuts_a_triangle_straddling_the_near_plane_at_z_zero() {
    let tri = [
        clip(0.0, 0.0, -1.0, 1.0),
        clip(1.0, 0.0, 1.0, 1.0),
        clip(0.0, 1.0, 0.5, 1.0),
    ];
    let polygon = clip_triangle_depth(tri);
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
    let polygon = clip_triangle_depth(tri);
    assert!(polygon.len() >= 3);
    for p in &polygon {
        assert!(p.z >= 0.0 && p.z <= p.w, "{p:?}");
    }
    assert!(polygon.iter().any(|p| p.z == p.w));
}
