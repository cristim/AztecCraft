//! Original vector UI icons, drawn in code on a 24-unit grid. Each glyph is
//! authored from a written spec; nothing is traced, copied or derived from any Maya asset.

use eframe::egui::{
    self, Color32, Pos2, Rect, Shape, Stroke, Vec2,
    epaint::{PathShape, PathStroke},
    pos2, vec2,
};
use std::f32::consts::{FRAC_1_SQRT_2, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiIcon {
    Select,
    Lasso,
    PaintSelect,
    Move,
    Rotate,
    Scale,
    /// Static placeholder glyph; the dynamic Last Used behaviour is unsupported.
    LastUsed,
    SelectHierarchy,
    SelectObject,
    SelectComponent,
    SelectVertex,
    SelectEdge,
    SelectFace,
    SidebarModelingToolkit,
    SidebarHumanIk,
    SidebarAttributeEditor,
    SidebarToolSettings,
    SidebarChannelBox,
    FileNew,
    FileOpen,
    FileSave,
    PolyCube,
    PolyPlane,
    PolySphere,
    PolyCylinder,
    PolyCone,
    Undo,
    Redo,
    FrameSelected,
}

/// Tool Box order.
pub const ALL: &[UiIcon] = &[
    UiIcon::Select,
    UiIcon::Lasso,
    UiIcon::PaintSelect,
    UiIcon::Move,
    UiIcon::Rotate,
    UiIcon::Scale,
    UiIcon::LastUsed,
    UiIcon::SelectHierarchy,
    UiIcon::SelectObject,
    UiIcon::SelectComponent,
    UiIcon::SelectVertex,
    UiIcon::SelectEdge,
    UiIcon::SelectFace,
    UiIcon::SidebarModelingToolkit,
    UiIcon::SidebarHumanIk,
    UiIcon::SidebarAttributeEditor,
    UiIcon::SidebarToolSettings,
    UiIcon::SidebarChannelBox,
    UiIcon::FileNew,
    UiIcon::FileOpen,
    UiIcon::FileSave,
    UiIcon::PolyCube,
    UiIcon::PolyPlane,
    UiIcon::PolySphere,
    UiIcon::PolyCylinder,
    UiIcon::PolyCone,
    UiIcon::Undo,
    UiIcon::Redo,
    UiIcon::FrameSelected,
];

// Newly chosen values; not sampled from any Maya image.
const NEUTRAL_LIGHT: Color32 = Color32::from_rgb(214, 218, 222);
const AXIS_X: Color32 = Color32::from_rgb(242, 98, 98);
const AXIS_Y: Color32 = Color32::from_rgb(124, 208, 96);
const AXIS_Z: Color32 = Color32::from_rgb(104, 152, 244);
const NEUTRAL_MID: Color32 = Color32::from_rgb(150, 156, 164);
const NEUTRAL_DARK: Color32 = Color32::from_rgb(106, 112, 122);
const ACCENT: Color32 = Color32::from_rgb(238, 196, 92);
const AMBER_LIGHT: Color32 = Color32::from_rgb(246, 200, 124);
const AMBER_MID: Color32 = Color32::from_rgb(232, 173, 85);
const AMBER_DARK: Color32 = Color32::from_rgb(168, 114, 52);
const AMBER_SHADOW: Color32 = Color32::from_rgb(120, 84, 44);
const CYAN_LIGHT: Color32 = Color32::from_rgb(126, 220, 230);
const CYAN_MID: Color32 = Color32::from_rgb(88, 201, 213);
const HISTORY_BLUE: Color32 = Color32::from_rgb(116, 169, 235);
const FRAME_VIOLET: Color32 = Color32::from_rgb(185, 154, 221);

const GRID: f32 = 24.0;
/// Stroke width in grid units.
const STROKE: f32 = 1.6;
/// The lasso's thinner-looking loop gets a heavier line to stay legible at 16 px.
const LASSO_STROKE: f32 = 1.3 * STROKE;
/// Rotate ring break gap, as a multiple of the stroke width.
const RING_GAP_STROKES: f32 = 1.375;
const SHAFT_HALF: f32 = 0.9;
const HEAD_LEN: f32 = 6.5;
const HEAD_HALF: f32 = 4.0;
const HANDLE: f32 = 6.8;
const CENTER_HALF: f32 = 1.9;
const CENTER: Pos2 = pos2(10.0, 14.0);
/// Centre of the 24-unit grid.
const GRID_CENTER: Pos2 = pos2(12.0, 12.0);
const X_DIR: Vec2 = vec2(1.0, 0.0);
const Y_DIR: Vec2 = vec2(0.0, -1.0);
const Z_DIR: Vec2 = vec2(-FRAC_1_SQRT_2, FRAC_1_SQRT_2);

/// Maps 24-unit grid geometry into a rect. Every filled shape is one closed path
/// that egui fans from vertex 0 (clockwise, star-shaped from vertex 0); overlapping
/// shapes are avoided because they double-composite under disabled opacity.
struct Fit {
    origin: Pos2,
    scale: f32,
}

impl Fit {
    fn new(rect: Rect) -> Self {
        Self {
            origin: rect.min,
            scale: rect.width().min(rect.height()) / GRID,
        }
    }

    fn map(&self, grid: &[Pos2]) -> Vec<Pos2> {
        grid.iter()
            .map(|p| self.origin + p.to_vec2() * self.scale)
            .collect()
    }

    fn fill(&self, grid: &[Pos2], color: Color32) -> Shape {
        let mut path = PathShape::closed_line(self.map(grid), Stroke::NONE);
        path.fill = color;
        Shape::Path(path)
    }

    fn stroke(&self, grid: &[Pos2], closed: bool, color: Color32, width: f32) -> Shape {
        let stroke = PathStroke::new(width * self.scale, color);
        let points = self.map(grid);
        Shape::Path(if closed {
            PathShape::closed_line(points, stroke)
        } else {
            PathShape::line(points, stroke)
        })
    }
}

fn rot(v: Vec2, angle: f32) -> Vec2 {
    let (s, c) = angle.sin_cos();
    vec2(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Points of an ellipse, increasing parameter (clockwise on screen).
fn ellipse(center: Pos2, radii: Vec2, tilt: f32, from: f32, sweep: f32, n: usize) -> Vec<Pos2> {
    (0..n)
        .map(|i| {
            let t = from + sweep * i as f32 / (n - 1) as f32;
            center + rot(vec2(radii.x * t.cos(), radii.y * t.sin()), tilt)
        })
        .collect()
}

fn disc(center: Pos2, r: f32) -> Vec<Pos2> {
    let mut pts = ellipse(center, vec2(r, r), 0.0, 0.0, TAU, 17);
    pts.pop();
    pts
}

/// Local (along, across) to grid; `across` is `dir` turned clockwise on screen.
fn along(origin: Pos2, dir: Vec2, pts: &[(f32, f32)]) -> Vec<Pos2> {
    let perp = vec2(-dir.y, dir.x);
    pts.iter()
        .map(|&(u, v)| origin + dir * u + perp * v)
        .collect()
}

/// Shaft and filled triangular head as one outline, starting at the tip.
fn arrow(origin: Pos2, dir: Vec2, from: f32, to: f32) -> Vec<Pos2> {
    arrow_with_shaft(origin, dir, from, to, SHAFT_HALF)
}

/// `arrow` with a chosen shaft half-width.
fn arrow_with_shaft(origin: Pos2, dir: Vec2, from: f32, to: f32, shaft_half: f32) -> Vec<Pos2> {
    let b = to - HEAD_LEN;
    let (w, h) = (shaft_half, HEAD_HALF);
    along(
        origin,
        dir,
        &[
            (to, 0.0),
            (b, h),
            (b, w),
            (from, w),
            (from, -w),
            (b, -w),
            (b, -h),
        ],
    )
}

/// Shaft ending in a square handle as one outline. Vertex 0 sits on the handle's
/// far edge inside the band that sees every vertex.
fn lollipop(origin: Pos2, dir: Vec2, from: f32, to: f32) -> Vec<Pos2> {
    let (w, c, a) = (SHAFT_HALF, HANDLE / 2.0, to - HANDLE);
    along(
        origin,
        dir,
        &[
            (to, 0.0),
            (to, c),
            (a, c),
            (a, w),
            (from, w),
            (from, -w),
            (a, -w),
            (a, -c),
            (to, -c),
        ],
    )
}

/// Point at `t` on the quadratic Bezier from `a` to `b` with control point `c`.
fn quad_bezier(a: Pos2, c: Pos2, b: Pos2, t: f32) -> Pos2 {
    let u = 1.0 - t;
    (a.to_vec2() * (u * u) + c.to_vec2() * (2.0 * t * u) + b.to_vec2() * (t * t)).to_pos2()
}

fn dist_to_polyline(p: Pos2, line: &[Pos2]) -> f32 {
    line.windows(2)
        .map(|s| {
            let d = s[1] - s[0];
            let t = ((p - s[0]).dot(d) / d.length_sq()).clamp(0.0, 1.0);
            (p - (s[0] + d * t)).length()
        })
        .fold(f32::MAX, f32::min)
}

/// Splits a closed ring into open runs, dropping points near `fronts`.
fn break_ring(ring: &[Pos2], fronts: &[&[Pos2]], gap: f32) -> Vec<Vec<Pos2>> {
    let keep: Vec<bool> = ring
        .iter()
        .map(|&p| fronts.iter().all(|f| dist_to_polyline(p, f) >= gap))
        .collect();
    let Some(start) = keep.iter().position(|k| !k) else {
        return vec![ring.to_vec()];
    };
    let mut runs: Vec<Vec<Pos2>> = vec![];
    let mut cur = vec![];
    for i in 1..=ring.len() {
        let j = (start + i) % ring.len();
        if keep[j] {
            cur.push(ring[j]);
        } else if !cur.is_empty() {
            runs.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        runs.push(cur);
    }
    runs.retain(|r| r.len() >= 2);
    runs
}

fn lasso(fit: &Fit) -> Vec<Shape> {
    let (c, radii, tilt) = (pos2(13.8, 9.8), vec2(9.0, 6.6), -0.4);
    let start = 3.3;
    let mut path = ellipse(c, radii, tilt, start, TAU - 1.0, 44);
    let p0 = path[0];
    // The cord leaves along the loop's tangent so the join stays smooth.
    let tangent = (path[1] - p0).normalized();
    let outward = (p0 - c).normalized();
    let ctrl = p0 - tangent * 4.0;
    let end = p0 - tangent * 5.5 + outward * 1.2;
    let knot = end + (end - ctrl).normalized() * 3.4;
    let cord: Vec<Pos2> = (0..8)
        .map(|i| quad_bezier(end, ctrl, p0, i as f32 / 7.0))
        .collect();
    path.splice(0..1, cord);
    vec![
        fit.stroke(&path, false, NEUTRAL_LIGHT, LASSO_STROKE),
        fit.fill(&disc(knot, 2.2), NEUTRAL_LIGHT),
    ]
}

fn paint_select(fit: &Fit) -> Vec<Shape> {
    let (b, r, half) = (pos2(16.5, 7.5), 4.4_f32, 1.5_f32);
    let d = vec2(FRAC_1_SQRT_2, -FRAC_1_SQRT_2);
    let junction = (half / r).asin();
    let far = 0.75 * PI; // opposite the handle
    let tip = -0.25 * PI;
    let mut pts = ellipse(b, vec2(r, r), 0.0, far, tip - junction + TAU - far, 12);
    pts.extend(along(b, d, &[(r + 2.4, -half), (r + 2.4, half)]));
    pts.extend(ellipse(
        b,
        vec2(r, r),
        0.0,
        tip + junction + TAU,
        far - tip - junction,
        12,
    ));
    pts.pop();
    let mut shapes = vec![fit.fill(&pts, ACCENT)];
    let (s, ctrl, e) = (b - d * 7.0, pos2(5.0, 12.0), pos2(3.2, 20.8));
    for i in 0..3 {
        let at = quad_bezier(s, ctrl, e, i as f32 / 2.0);
        shapes.push(fit.fill(&disc(at, 1.9), NEUTRAL_LIGHT));
    }
    shapes
}

fn axes(fit: &Fit, shape: fn(Pos2, Vec2, f32, f32) -> Vec<Pos2>, gaps: [f32; 3]) -> Vec<Shape> {
    [
        (X_DIR, AXIS_X, 13.0),
        (Y_DIR, AXIS_Y, 12.5),
        (Z_DIR, AXIS_Z, 10.9),
    ]
    .iter()
    .zip(gaps)
    .map(|(&(dir, color, to), from)| fit.fill(&shape(CENTER, dir, from, to), color))
    .collect()
}

fn rotate(fit: &Fit) -> Vec<Shape> {
    let c = GRID_CENTER;
    // 96 samples include the closing duplicate of point 0; keep the first 95.
    let ring = |radii: Vec2, tilt: f32| ellipse(c, radii, tilt, 0.0, TAU, 96)[..95].to_vec();
    let (z, y, x) = (
        ring(vec2(10.6, 10.6), 0.0),
        ring(vec2(8.2, 3.2), 0.0),
        ring(vec2(4.4, 8.2), 0.3),
    );
    let gap = RING_GAP_STROKES * STROKE;
    let mut shapes = vec![];
    for (pts, color, fronts) in [
        (&z, AXIS_Z, vec![&x[..], &y[..]]),
        (&x, AXIS_X, vec![&y[..]]),
        (&y, AXIS_Y, vec![]),
    ] {
        for run in break_ring(pts, &fronts, gap) {
            let closed = run.len() == pts.len();
            shapes.push(fit.stroke(&run, closed, color, STROKE));
        }
    }
    shapes
}

fn last_used(fit: &Fit) -> Vec<Shape> {
    let (c, r) = (pos2(17.6, 6.8), 4.7_f32);
    let cut = r + 3.2;
    let (lo, hi) = (2.5, 16.5);
    let (top, bottom) = (8.0, 22.0);
    // Rounded square, clockwise from the top-right corner, so the cut is one run.
    let rr = 2.6_f32;
    let corner =
        |cx: f32, cy: f32, from: f32| ellipse(pos2(cx, cy), vec2(rr, rr), 0.0, from, 0.5 * PI, 7);
    let edge = |a: Pos2, b: Pos2| (1..12).map(move |i| a + (b - a) * (i as f32 / 12.0));
    let ring: Vec<Pos2> = [
        corner(hi - rr, top + rr, -0.5 * PI),
        edge(pos2(hi, top + rr), pos2(hi, bottom - rr)).collect(),
        corner(hi - rr, bottom - rr, 0.0),
        edge(pos2(hi - rr, bottom), pos2(lo + rr, bottom)).collect(),
        corner(lo + rr, bottom - rr, 0.5 * PI),
        edge(pos2(lo, bottom - rr), pos2(lo, top + rr)).collect(),
        corner(lo + rr, top + rr, PI),
        edge(pos2(lo + rr, top), pos2(hi - rr, top)).collect(),
    ]
    .concat()
    .into_iter()
    .filter(|p| (*p - c).length() >= cut)
    .collect();
    let end = 0.75 * PI;
    let arc = ellipse(c, vec2(r, r), 0.0, 1.25 * PI, 1.5 * PI, 32);
    let tangent = vec2(-end.sin(), end.cos());
    let normal = vec2(end.cos(), end.sin());
    let base = c + normal * r + tangent * 1.3;
    let head = [
        base + tangent * 3.4,
        base - normal * 2.2,
        base + normal * 2.2,
    ];
    vec![
        fit.stroke(&ring, false, NEUTRAL_LIGHT, STROKE),
        fit.stroke(&arc, false, NEUTRAL_LIGHT, STROKE),
        fit.fill(&head, NEUTRAL_LIGHT),
    ]
}

/// Rectangle from `a` to `b` as one clockwise outline.
fn rect_path(a: Pos2, b: Pos2) -> [Pos2; 4] {
    [a, pos2(b.x, a.y), b, pos2(a.x, b.y)]
}

/// Gap between neighbouring regions, in grid units.
const REGION_GAP: f32 = 0.7;
/// Hierarchy node disc radius.
const NODE_RADIUS: f32 = 2.8;
/// Half the width of a hierarchy connector bar.
const BAR_HALF: f32 = 1.2;
/// Top edge of the hierarchy fork bar.
const FORK_BAR_TOP: f32 = 11.6;

fn select_hierarchy(fit: &Fit) -> Vec<Shape> {
    let (root, left, right) = (pos2(12.0, 4.4), pos2(5.0, 20.4), pos2(19.0, 20.4));
    let (r, half) = (NODE_RADIUS, BAR_HALF);
    let (stem_top, bar_top) = (root.y + r + REGION_GAP, FORK_BAR_TOP);
    let bar_bottom = bar_top + 2.0 * half;
    let drop_bottom = left.y - r - REGION_GAP;
    let (stem_bottom, drop_top) = (bar_top - REGION_GAP, bar_bottom + REGION_GAP);
    let mut shapes = vec![
        fit.fill(&disc(root, r), ACCENT),
        fit.fill(
            &rect_path(
                pos2(root.x - half, stem_top),
                pos2(root.x + half, stem_bottom),
            ),
            NEUTRAL_MID,
        ),
        fit.fill(
            &rect_path(
                pos2(left.x - half, bar_top),
                pos2(right.x + half, bar_bottom),
            ),
            NEUTRAL_MID,
        ),
    ];
    for node in [left, right] {
        let drop = rect_path(
            pos2(node.x - half, drop_top),
            pos2(node.x + half, drop_bottom),
        );
        shapes.push(fit.fill(&drop, NEUTRAL_MID));
    }
    shapes.push(fit.fill(&disc(left, r), NEUTRAL_LIGHT));
    shapes.push(fit.fill(&disc(right, r), NEUTRAL_LIGHT));
    shapes
}

/// Cube silhouette vertices: six hexagon corners clockwise from the top, then the centre.
fn cube_vertices(c: Pos2, radius: f32) -> [Pos2; 7] {
    let corner = |k: usize| {
        let a = k as f32 * PI / 3.0;
        c + vec2(radius * a.sin(), -radius * a.cos())
    };
    [
        corner(0),
        corner(1),
        corner(2),
        corner(3),
        corner(4),
        corner(5),
        c,
    ]
}

/// Moves every edge of a clockwise convex polygon inward by `d`.
fn inset(poly: &[Pos2], d: f32) -> Vec<Pos2> {
    let n = poly.len();
    // Each edge as a point and direction, pushed along its inward normal.
    let lines: Vec<(Pos2, Vec2)> = (0..n)
        .map(|i| {
            let dir = (poly[(i + 1) % n] - poly[i]).normalized();
            (poly[i] + vec2(-dir.y, dir.x) * d, dir)
        })
        .collect();
    (0..n)
        .map(|i| {
            let (p, u) = lines[(i + n - 1) % n];
            let (q, v) = lines[i];
            let t = ((q - p).x * v.y - (q - p).y * v.x) / (u.x * v.y - u.y * v.x);
            p + u * t
        })
        .collect()
}

fn select_object(fit: &Fit) -> Vec<Shape> {
    shaded_cube(fit, GRID_CENTER, OBJECT_CUBE_RADIUS)
}

/// A cube as three shaded rhombus regions separated by `REGION_GAP`.
fn shaded_cube(fit: &Fit, center: Pos2, radius: f32) -> Vec<Shape> {
    tinted_cube(
        fit,
        center,
        radius,
        [NEUTRAL_LIGHT, NEUTRAL_MID, NEUTRAL_DARK],
    )
}

/// `shaded_cube` with chosen top, right and left face colours.
fn tinted_cube(fit: &Fit, center: Pos2, radius: f32, tones: [Color32; 3]) -> Vec<Shape> {
    let [top, ur, lr, bottom, ll, ul, c] = cube_vertices(center, radius);
    let half_gap = REGION_GAP / 2.0;
    [
        ([top, ur, c, ul], tones[0]),
        ([c, ur, lr, bottom], tones[1]),
        ([c, bottom, ll, ul], tones[2]),
    ]
    .iter()
    .map(|(face, color)| fit.fill(&inset(face, half_gap), *color))
    .collect()
}

/// Cube circumradii, in grid units.
const OBJECT_CUBE_RADIUS: f32 = 11.4;
const COMPONENT_CUBE_RADIUS: f32 = 10.0;
/// Half the width of a cube edge bar.
const EDGE_HALF: f32 = 0.6;
/// Half the side of a cube corner point.
const POINT_HALF: f32 = 1.4;

fn select_component(fit: &Fit) -> Vec<Shape> {
    let v = cube_vertices(GRID_CENTER, COMPONENT_CUBE_RADIUS);
    let [top, ur, lr, bottom, ll, ul, c] = v;
    let edges = [
        (top, ur),
        (ur, lr),
        (lr, bottom),
        (bottom, ll),
        (ll, ul),
        (ul, top),
        (c, ur),
        (c, bottom),
        (c, ul),
    ];
    let mut shapes = vec![];
    for (a, b) in edges {
        let dir = (b - a).normalized();
        // Distance from a corner point's centre to its far side along the edge, plus the gap.
        let clear = POINT_HALF * (dir.x.abs() + dir.y.abs()) + REGION_GAP;
        let len = (b - a).length();
        shapes.push(fit.fill(
            &along(
                a,
                dir,
                &[
                    (clear, -EDGE_HALF),
                    (len - clear, -EDGE_HALF),
                    (len - clear, EDGE_HALF),
                    (clear, EDGE_HALF),
                ],
            ),
            NEUTRAL_LIGHT,
        ));
    }
    for p in v {
        let h = POINT_HALF;
        shapes.push(fit.fill(&rect_path(p - vec2(h, h), p + vec2(h, h)), ACCENT));
    }
    shapes
}

/// The tilted quad shared by the Vertex, Edge and Face glyphs, clockwise from the top left.
const COMPONENT_QUAD: [Pos2; 4] = [
    pos2(3.2, 6.0),
    pos2(18.0, 3.2),
    pos2(20.8, 18.0),
    pos2(6.0, 20.8),
];
/// The Edge and Face glyphs use the quad grown about the grid centre to span the Vertex points.
const QUAD_GROWTH: f32 = 1.24;
/// Half the side of a Vertex glyph point.
const QUAD_POINT_HALF: f32 = 2.8;
/// Half the width of an Edge glyph bar, and of its highlighted bar.
const QUAD_EDGE_HALF: f32 = 1.0;
const QUAD_EDGE_HIGHLIGHT_HALF: f32 = 1.6;
/// How far an Edge glyph bar stops short of each quad corner.
const QUAD_EDGE_CLEAR: f32 = 1.8;
/// Gap between the Face glyph's trapezoids; wider than `REGION_GAP` so the joints survive 16 px.
const FACE_GAP: f32 = 1.3;
/// Scale of the Face glyph's hole relative to the grown quad.
const FACE_HOLE: f32 = 0.5;
/// Scale of the Face glyph's centre handle relative to the grown quad.
const FACE_HANDLE: f32 = 0.2;

/// Four corner points of the quad, the first one accent-coloured as the selected point.
fn select_vertex(fit: &Fit) -> Vec<Shape> {
    COMPONENT_QUAD
        .iter()
        .enumerate()
        .map(|(i, &p)| {
            let h = QUAD_POINT_HALF;
            let color = if i == 0 { ACCENT } else { NEUTRAL_LIGHT };
            fit.fill(&rect_path(p - vec2(h, h), p + vec2(h, h)), color)
        })
        .collect()
}

fn grown_quad() -> [Pos2; 4] {
    COMPONENT_QUAD.map(|p| GRID_CENTER + (p - GRID_CENTER) * QUAD_GROWTH)
}

/// Four separate quad edges, the right one thicker and accent-coloured as the selected edge.
fn select_edge(fit: &Fit) -> Vec<Shape> {
    let quad = grown_quad();
    (0..4)
        .map(|i| {
            let (a, b) = (quad[i], quad[(i + 1) % 4]);
            let dir = (b - a).normalized();
            let len = (b - a).length();
            let (half, color) = if i == 1 {
                (QUAD_EDGE_HIGHLIGHT_HALF, ACCENT)
            } else {
                (QUAD_EDGE_HALF, NEUTRAL_LIGHT)
            };
            let (from, to) = (QUAD_EDGE_CLEAR, len - QUAD_EDGE_CLEAR);
            fit.fill(
                &along(
                    a,
                    dir,
                    &[(from, -half), (to, -half), (to, half), (from, half)],
                ),
                color,
            )
        })
        .collect()
}

/// The quad as a facet: four trapezoids around a centre point, like a shaded polygon with its
/// face handle.
fn select_face(fit: &Fit) -> Vec<Shape> {
    let outer = grown_quad();
    let inner = outer.map(|p| GRID_CENTER + (p - GRID_CENTER) * FACE_HOLE);
    let half_gap = FACE_GAP / 2.0;
    let mut shapes: Vec<Shape> = (0..4)
        .map(|i| {
            let j = (i + 1) % 4;
            let piece = [outer[i], outer[j], inner[j], inner[i]];
            fit.fill(&inset(&piece, half_gap), AMBER_MID)
        })
        .collect();
    let handle = outer.map(|p| GRID_CENTER + (p - GRID_CENTER) * FACE_HANDLE);
    shapes.push(fit.fill(&handle, NEUTRAL_LIGHT));
    shapes
}

/// One filled rectangle region per `([x0, y0, x1, y1], colour)` entry, in grid units.
fn rects(fit: &Fit, list: &[([f32; 4], Color32)]) -> Vec<Shape> {
    list.iter()
        .map(|&([x0, y0, x1, y1], color)| fit.fill(&rect_path(pos2(x0, y0), pos2(x1, y1)), color))
        .collect()
}

/// Left and right edge of the panel glyphs' content, in grid units.
const PANEL_LEFT: f32 = 2.0;
const PANEL_RIGHT: f32 = 22.0;
/// Half-height of an Attribute Editor label bar.
const LABEL_HALF: f32 = 1.2;
/// Modeling Toolkit list rows: pitch and height are multiples of 1.5 units, one pixel at 16 px.
const LIST_PITCH: f32 = 3.0;
const LIST_ROW_HEIGHT: f32 = 1.5;
const LIST_TOP: f32 = 12.0;
/// Chip width and the clear space between a row's chip and bar.
const CHIP_WIDTH: f32 = 3.0;
const CHIP_BAR_GAP: f32 = 1.5;

fn sidebar_modeling_toolkit(fit: &Fit) -> Vec<Shape> {
    let mut shapes = shaded_cube(fit, pos2(12.0, 5.4), 5.0);
    let mut list = vec![];
    for k in 0..3 {
        let top = LIST_TOP + LIST_PITCH * k as f32;
        let bottom = top + LIST_ROW_HEIGHT;
        let chip_right = PANEL_LEFT + CHIP_WIDTH;
        list.push(([PANEL_LEFT, top, chip_right, bottom], ACCENT));
        list.push((
            [chip_right + CHIP_BAR_GAP, top, PANEL_RIGHT, bottom],
            NEUTRAL_MID,
        ));
    }
    shapes.extend(rects(fit, &list));
    shapes
}

fn sidebar_human_ik(fit: &Fit) -> Vec<Shape> {
    let mut shapes = vec![fit.fill(&disc(pos2(12.0, 4.2), 2.6), ACCENT)];
    let (torso_l, torso_r) = (9.4, 14.6);
    let (torso_top, torso_bottom) = (7.4, 14.4);
    let leg_top = torso_bottom + REGION_GAP;
    shapes.extend(rects(
        fit,
        &[
            ([torso_l, torso_top, torso_r, torso_bottom], NEUTRAL_LIGHT),
            ([3.0, torso_top, torso_l - REGION_GAP, 9.8], NEUTRAL_MID),
            ([torso_r + REGION_GAP, torso_top, 21.0, 9.8], NEUTRAL_MID),
            ([8.6, leg_top, 11.0, 22.4], NEUTRAL_MID),
            ([13.0, leg_top, 15.4, 22.4], NEUTRAL_MID),
        ],
    ));
    shapes
}

/// Right edge of each Attribute Editor value box; ragged on purpose.
const VALUE_RIGHT: [f32; 3] = [22.0, 17.0, 20.0];

fn sidebar_attribute_editor(fit: &Fit) -> Vec<Shape> {
    let mut list = vec![];
    for (k, right) in VALUE_RIGHT.into_iter().enumerate() {
        let c = 5.4 + 6.6 * k as f32;
        list.push((
            [PANEL_LEFT, c - LABEL_HALF, 9.0, c + LABEL_HALF],
            NEUTRAL_LIGHT,
        ));
        list.push(([10.4, c - 2.3, right, c + 2.3], NEUTRAL_MID));
    }
    rects(fit, &list)
}

/// Half-size of a slider knob, and its (centre x, centre y) per track.
const KNOB_HALF: f32 = 2.2;
const TRACK_HALF: f32 = 1.0;
const SLIDERS: [(f32, f32); 3] = [(7.0, 5.0), (15.0, 12.0), (10.5, 19.0)];

fn sidebar_tool_settings(fit: &Fit) -> Vec<Shape> {
    let mut list = vec![];
    for (kx, cy) in SLIDERS {
        let (lo, hi) = (kx - KNOB_HALF, kx + KNOB_HALF);
        list.push((
            [
                PANEL_LEFT,
                cy - TRACK_HALF,
                lo - REGION_GAP,
                cy + TRACK_HALF,
            ],
            NEUTRAL_MID,
        ));
        list.push(([lo, cy - KNOB_HALF, hi, cy + KNOB_HALF], ACCENT));
        list.push((
            [
                hi + REGION_GAP,
                cy - TRACK_HALF,
                PANEL_RIGHT,
                cy + TRACK_HALF,
            ],
            NEUTRAL_MID,
        ));
    }
    rects(fit, &list)
}

fn sidebar_channel_box(fit: &Fit) -> Vec<Shape> {
    let mut list = vec![];
    for k in 0..3 {
        let top = 2.5 + 4.0 * k as f32;
        list.push(([PANEL_LEFT, top, 9.4, top + 3.0], NEUTRAL_LIGHT));
        list.push(([10.6, top, PANEL_RIGHT, top + 3.0], NEUTRAL_MID));
    }
    list.push(([PANEL_LEFT, 14.5, PANEL_RIGHT, 15.9], NEUTRAL_DARK));
    list.push(([PANEL_LEFT, 18.6, PANEL_RIGHT, 21.4], ACCENT));
    rects(fit, &list)
}

/// File Save tray edges, in grid units.
const TRAY_LEFT: f32 = 3.0;
const TRAY_RIGHT: f32 = 21.0;

/// File New page edges and folded-corner size, in grid units.
const NEW_LEFT: f32 = 3.0;
const NEW_RIGHT: f32 = 21.0;
const NEW_TOP: f32 = 1.5;
const NEW_BOTTOM: f32 = 22.5;
const NEW_FOLD: f32 = 5.5;
/// File New plus badge: centre, arm half-length and bar half-thickness.
const NEW_PLUS_CENTER: Pos2 = Pos2 { x: 16.5, y: 17.5 };
const NEW_PLUS_ARM: f32 = 4.5;
const NEW_PLUS_HALF: f32 = 1.6;
/// Clearance between the plus badge and the page edges, wider than `REGION_GAP` so
/// the cross stays separate from the page at 16 px.
const NEW_NOTCH_CLEAR: f32 = 1.5;

fn file_new(fit: &Fit) -> Vec<Shape> {
    let g = REGION_GAP;
    let (fold_x, fold_y) = (NEW_RIGHT - NEW_FOLD, NEW_TOP + NEW_FOLD);
    let (cx, cy, arm, half) = (
        NEW_PLUS_CENTER.x,
        NEW_PLUS_CENTER.y,
        NEW_PLUS_ARM,
        NEW_PLUS_HALF,
    );
    let (notch_left, notch_top) = (cx - arm - NEW_NOTCH_CLEAR, cy - arm - NEW_NOTCH_CLEAR);
    // Vertex 0 sits on the left edge, below the fold step and above the notch, so
    // the fan reaches every corner of the L.
    let start = pos2(NEW_LEFT, fold_y + 2.0 * g);
    let page = [
        start,
        pos2(NEW_LEFT, NEW_TOP),
        pos2(fold_x - g, NEW_TOP),
        pos2(fold_x - g, fold_y + g),
        pos2(NEW_RIGHT, fold_y + g),
        pos2(NEW_RIGHT, notch_top),
        pos2(notch_left, notch_top),
        pos2(notch_left, NEW_BOTTOM),
        pos2(NEW_LEFT, NEW_BOTTOM),
    ];
    let mut shapes = vec![fit.fill(&page, NEUTRAL_LIGHT)];
    shapes.extend(rects(
        fit,
        &[
            ([cx - arm, cy - half, cx + arm, cy + half], ACCENT),
            ([cx - half, cy - arm, cx + half, cy - half - g], ACCENT),
            ([cx - half, cy + half + g, cx + half, cy + arm], ACCENT),
        ],
    ));
    shapes.push(fit.fill(
        &[
            pos2(fold_x, NEW_TOP),
            pos2(NEW_RIGHT, fold_y),
            pos2(fold_x, fold_y),
        ],
        NEUTRAL_MID,
    ));
    shapes
}

/// File Open folder: tab top and bottom, body bottom, and the slanted flap's lean.
const FOLDER_TAB_TOP: f32 = 3.0;
const FOLDER_TAB_BOTTOM: f32 = 7.5;
const FOLDER_BODY_BOTTOM: f32 = 13.0;
const FOLDER_BOTTOM: f32 = 21.5;
const FOLDER_LEAN: f32 = 4.0;
/// File Open body edges, tab width and how far the flap overhangs the body.
const FOLDER_LEFT: f32 = 2.5;
const FOLDER_RIGHT: f32 = 21.5;
const FOLDER_TAB_RIGHT: f32 = 10.5;
const FOLDER_FLAP_OVERHANG: f32 = 1.0;

fn file_open(fit: &Fit) -> Vec<Shape> {
    let (left, right, tab_right) = (FOLDER_LEFT, FOLDER_RIGHT, FOLDER_TAB_RIGHT);
    let flap_right = right + FOLDER_FLAP_OVERHANG;
    let g = REGION_GAP;
    let body_top = FOLDER_TAB_BOTTOM + g;
    let flap_top = FOLDER_BODY_BOTTOM + g;
    let mut shapes = rects(
        fit,
        &[
            (
                [left, FOLDER_TAB_TOP, tab_right, FOLDER_TAB_BOTTOM],
                NEUTRAL_MID,
            ),
            ([left, body_top, right, FOLDER_BODY_BOTTOM], NEUTRAL_LIGHT),
        ],
    );
    shapes.push(fit.fill(
        &[
            pos2(left + FOLDER_LEAN, flap_top),
            pos2(flap_right, flap_top),
            pos2(flap_right - FOLDER_LEAN, FOLDER_BOTTOM),
            pos2(left, FOLDER_BOTTOM),
        ],
        ACCENT,
    ));
    shapes
}

/// File Save tray: arm height and thickness, and where the arrow starts and ends.
const TRAY_TOP: f32 = 12.5;
const TRAY_BOTTOM: f32 = 21.5;
const TRAY_THICK: f32 = 2.8;
/// Half the File Save arrow's shaft width.
const SAVE_SHAFT_HALF: f32 = 1.5;
const SAVE_ARROW_TOP: f32 = 1.5;
const SAVE_ARROW_TIP: f32 = 17.2;

fn file_save(fit: &Fit) -> Vec<Shape> {
    let (left, right, g) = (TRAY_LEFT, TRAY_RIGHT, REGION_GAP);
    let mut shapes = rects(
        fit,
        &[
            (
                [left, TRAY_TOP, left + TRAY_THICK, TRAY_BOTTOM],
                NEUTRAL_LIGHT,
            ),
            (
                [
                    left + TRAY_THICK + g,
                    TRAY_BOTTOM - TRAY_THICK,
                    right - TRAY_THICK - g,
                    TRAY_BOTTOM,
                ],
                NEUTRAL_LIGHT,
            ),
            (
                [right - TRAY_THICK, TRAY_TOP, right, TRAY_BOTTOM],
                NEUTRAL_LIGHT,
            ),
        ],
    );
    let down = vec2(0.0, 1.0);
    shapes.push(fit.fill(
        &arrow_with_shaft(
            pos2(GRID_CENTER.x, 0.0),
            down,
            SAVE_ARROW_TOP,
            SAVE_ARROW_TIP,
            SAVE_SHAFT_HALF,
        ),
        ACCENT,
    ));
    shapes
}

/// Poly Cube: a smaller amber cube lifted over a ground-shadow bar.
const POLY_CUBE_CENTER: Pos2 = pos2(12.0, 10.9);
const POLY_CUBE_RADIUS: f32 = 9.4;
const SHADOW_TOP: f32 = 21.0;
const SHADOW_BOTTOM: f32 = 23.0;
const SHADOW_HALF_WIDTH: f32 = 9.5;

fn poly_cube(fit: &Fit) -> Vec<Shape> {
    let mut shapes = tinted_cube(
        fit,
        POLY_CUBE_CENTER,
        POLY_CUBE_RADIUS,
        [AMBER_LIGHT, AMBER_MID, AMBER_DARK],
    );
    let (l, r) = (
        GRID_CENTER.x - SHADOW_HALF_WIDTH,
        GRID_CENTER.x + SHADOW_HALF_WIDTH,
    );
    shapes.push(fit.fill(
        &rect_path(pos2(l, SHADOW_TOP), pos2(r, SHADOW_BOTTOM)),
        AMBER_SHADOW,
    ));
    shapes
}

/// Poly Plane corners (top-left, top-right, bottom-right, bottom-left): a parallelogram
/// leaning right, tiled 2x2.
const PLANE_CORNERS: [Pos2; 4] = [
    pos2(7.0, 3.0),
    pos2(23.0, 3.0),
    pos2(17.0, 21.0),
    pos2(1.0, 21.0),
];

/// Poly Plane tiles: each spans half the plane, starting at 0 or half.
const PLANE_TILE: f32 = 0.5;
const PLANE_TILE_STARTS: [f32; 2] = [0.0, PLANE_TILE];

fn poly_plane(fit: &Fit) -> Vec<Shape> {
    let [tl, tr, br, bl] = PLANE_CORNERS;
    let at = |u: f32, v: f32| {
        let top = tl + (tr - tl) * u;
        let bottom = bl + (br - bl) * u;
        top + (bottom - top) * v
    };
    let mut shapes = vec![];
    for (row, v) in PLANE_TILE_STARTS.into_iter().enumerate() {
        for (col, u) in PLANE_TILE_STARTS.into_iter().enumerate() {
            let quad = [
                at(u, v),
                at(u + PLANE_TILE, v),
                at(u + PLANE_TILE, v + PLANE_TILE),
                at(u, v + PLANE_TILE),
            ];
            let tone = if (row + col) % 2 == 0 {
                CYAN_LIGHT
            } else {
                CYAN_MID
            };
            shapes.push(fit.fill(&inset(&quad, REGION_GAP / 2.0), tone));
        }
    }
    shapes
}

fn poly_sphere(fit: &Fit) -> Vec<Shape> {
    vec![
        fit.fill(
            &[
                pos2(1.4, 10.5),
                pos2(3.5, 5.2),
                pos2(8.0, 1.4),
                pos2(16.0, 1.4),
                pos2(20.5, 5.2),
                pos2(22.6, 10.5),
            ],
            AMBER_LIGHT,
        ),
        fit.fill(
            &[
                pos2(1.4, 13.5),
                pos2(22.6, 13.5),
                pos2(20.5, 18.8),
                pos2(16.0, 22.6),
                pos2(8.0, 22.6),
                pos2(3.5, 18.8),
            ],
            AMBER_MID,
        ),
    ]
}

fn poly_cylinder(fit: &Fit) -> Vec<Shape> {
    vec![
        fit.fill(
            &[
                pos2(2.1, 5.0),
                pos2(6.0, 1.6),
                pos2(18.0, 1.6),
                pos2(21.9, 5.0),
                pos2(18.0, 8.0),
                pos2(6.0, 8.0),
            ],
            AMBER_LIGHT,
        ),
        fit.fill(
            &[
                pos2(2.1, 10.5),
                pos2(21.9, 10.5),
                pos2(21.9, 19.5),
                pos2(17.0, 22.4),
                pos2(7.0, 22.4),
                pos2(2.1, 19.5),
            ],
            AMBER_MID,
        ),
    ]
}

fn poly_cone(fit: &Fit) -> Vec<Shape> {
    vec![fit.fill(
        &[
            pos2(12.0, 1.3),
            pos2(22.7, 20.5),
            pos2(17.0, 22.7),
            pos2(7.0, 22.7),
            pos2(1.3, 20.5),
        ],
        AMBER_MID,
    )]
}

/// History arrow arc: ellipse centre and radii, and the parameter range swept from the
/// right end, over the top, to the left end.
const HISTORY_ARC_CENTER: Pos2 = pos2(12.8, 13.0);
const HISTORY_ARC_RADII: Vec2 = vec2(10.0, 9.4);
const HISTORY_ARC_START: f32 = 0.3 * PI;
const HISTORY_ARC_END: f32 = -0.62 * PI;
const HISTORY_ARC_POINTS: usize = 32;
const HISTORY_STROKE: f32 = 1.8 * STROKE;
/// Clear space between the arc's end and the arrowhead base, and the head's size.
const HISTORY_HEAD_GAP: f32 = 2.6;
const HISTORY_HEAD_LEN: f32 = 6.0;
const HISTORY_HEAD_HALF: f32 = 4.2;

/// Undo geometry in grid units: the open arc, and the head as a clockwise fan.
fn undo_parts() -> (Vec<Pos2>, [Pos2; 3]) {
    let arc = ellipse(
        HISTORY_ARC_CENTER,
        HISTORY_ARC_RADII,
        0.0,
        HISTORY_ARC_START,
        HISTORY_ARC_END - HISTORY_ARC_START,
        HISTORY_ARC_POINTS,
    );
    let end = arc[HISTORY_ARC_POINTS - 1];
    let dir = (end - arc[HISTORY_ARC_POINTS - 2]).normalized();
    let base = end + dir * HISTORY_HEAD_GAP;
    let head = along(
        base,
        dir,
        &[
            (HISTORY_HEAD_LEN, 0.0),
            (0.0, HISTORY_HEAD_HALF),
            (0.0, -HISTORY_HEAD_HALF),
        ],
    );
    (arc, [head[0], head[1], head[2]])
}

/// Mirrors grid points left to right about the grid centre.
fn mirror_x(p: Pos2) -> Pos2 {
    pos2(GRID - p.x, p.y)
}

/// Restores clockwise winding after a one-axis flip: keeps vertex 0, reverses the rest.
fn reverse_winding(fan: &mut [Pos2]) {
    fan[1..].reverse();
}

fn history(fit: &Fit, mirrored: bool) -> Vec<Shape> {
    let (mut arc, mut head) = undo_parts();
    if mirrored {
        arc.iter_mut().for_each(|p| *p = mirror_x(*p));
        head.iter_mut().for_each(|p| *p = mirror_x(*p));
        reverse_winding(&mut head);
    }
    vec![
        fit.stroke(&arc, false, HISTORY_BLUE, HISTORY_STROKE),
        fit.fill(&head, HISTORY_BLUE),
    ]
}

/// Frame Selected: corner brackets around a centred square.
const FRAME_OUTER: f32 = 2.0;
const FRAME_ARM: f32 = 7.0;
const FRAME_THICK: f32 = 2.4;
const FRAME_SQUARE_HALF: f32 = 3.0;

fn frame_selected(fit: &Fit) -> Vec<Shape> {
    let (arm, t) = (FRAME_ARM, FRAME_THICK);
    // Top-left bracket, clockwise from its outer corner.
    let local = [
        (0.0, 0.0),
        (arm, 0.0),
        (arm, t),
        (t, t),
        (t, arm),
        (0.0, arm),
    ];
    let mut shapes = vec![];
    for (sx, sy) in [(1.0, 1.0), (-1.0, 1.0), (-1.0, -1.0), (1.0, -1.0)] {
        let corner = pos2(
            GRID_CENTER.x - sx * (GRID_CENTER.x - FRAME_OUTER),
            GRID_CENTER.y - sy * (GRID_CENTER.y - FRAME_OUTER),
        );
        let mut pts: Vec<Pos2> = local
            .iter()
            .map(|&(u, v)| corner + vec2(sx * u, sy * v))
            .collect();
        if sx * sy < 0.0 {
            reverse_winding(&mut pts);
        }
        shapes.push(fit.fill(&pts, FRAME_VIOLET));
    }
    let h = FRAME_SQUARE_HALF;
    shapes.push(fit.fill(
        &rect_path(GRID_CENTER - vec2(h, h), GRID_CENTER + vec2(h, h)),
        FRAME_VIOLET,
    ));
    shapes
}

/// Geometry of `icon` fitted to `rect`, without needing a window. Disabled looks
/// come from the host `Ui` (egui's disabled opacity); icons have no tone.
pub fn shapes(icon: UiIcon, rect: Rect) -> Vec<Shape> {
    let fit = Fit::new(rect);
    match icon {
        // Solid arrow pointer, tip upper-left, angled down-right, short tail.
        UiIcon::Select => vec![fit.fill(
            &[
                pos2(2.5, 2.0),   // tip
                pos2(22.0, 13.5), // wing
                pos2(10.5, 13.5), // notch
                pos2(13.5, 20.5), // tail right
                pos2(12.0, 23.0), // tail end
                pos2(7.5, 14.5),  // tail left
                pos2(2.5, 19.5),  // heel
            ],
            NEUTRAL_LIGHT,
        )],
        UiIcon::Lasso => lasso(&fit),
        UiIcon::PaintSelect => paint_select(&fit),
        UiIcon::Move => axes(&fit, arrow, [1.5, 1.5, 1.5]),
        UiIcon::Rotate => rotate(&fit),
        UiIcon::Scale => {
            let mut shapes = axes(&fit, lollipop, [3.1, 3.1, 3.7]);
            let h = CENTER_HALF;
            let square = [(h, -h), (h, h), (-h, h), (-h, -h)];
            shapes.push(fit.fill(&along(CENTER, X_DIR, &square), NEUTRAL_LIGHT));
            shapes
        }
        UiIcon::LastUsed => last_used(&fit),
        UiIcon::SelectHierarchy => select_hierarchy(&fit),
        UiIcon::SelectObject => select_object(&fit),
        UiIcon::SelectComponent => select_component(&fit),
        UiIcon::SelectVertex => select_vertex(&fit),
        UiIcon::SelectEdge => select_edge(&fit),
        UiIcon::SelectFace => select_face(&fit),
        UiIcon::SidebarModelingToolkit => sidebar_modeling_toolkit(&fit),
        UiIcon::SidebarHumanIk => sidebar_human_ik(&fit),
        UiIcon::SidebarAttributeEditor => sidebar_attribute_editor(&fit),
        UiIcon::SidebarToolSettings => sidebar_tool_settings(&fit),
        UiIcon::SidebarChannelBox => sidebar_channel_box(&fit),
        UiIcon::FileNew => file_new(&fit),
        UiIcon::FileOpen => file_open(&fit),
        UiIcon::FileSave => file_save(&fit),
        UiIcon::PolyCube => poly_cube(&fit),
        UiIcon::PolyPlane => poly_plane(&fit),
        UiIcon::PolySphere => poly_sphere(&fit),
        UiIcon::PolyCylinder => poly_cylinder(&fit),
        UiIcon::PolyCone => poly_cone(&fit),
        UiIcon::Undo => history(&fit, false),
        UiIcon::Redo => history(&fit, true),
        UiIcon::FrameSelected => frame_selected(&fit),
    }
}

pub fn paint(painter: &egui::Painter, rect: Rect, icon: UiIcon) {
    painter.extend(shapes(icon, rect));
}

#[cfg(test)]
mod tests;
