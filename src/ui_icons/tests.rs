use super::*;
use eframe::egui::epaint::{ClippedShape, ColorMode, Primitive, TessellationOptions, Tessellator};

const PANEL: [f32; 3] = [43.0, 43.0, 43.0];
const MARGIN: f32 = 4.0;

fn index(icon: UiIcon) -> usize {
    match icon {
        UiIcon::Select => 0,
        UiIcon::Lasso => 1,
        UiIcon::PaintSelect => 2,
        UiIcon::Move => 3,
        UiIcon::Rotate => 4,
        UiIcon::Scale => 5,
        UiIcon::LastUsed => 6,
        UiIcon::SelectHierarchy => 7,
        UiIcon::SelectObject => 8,
        UiIcon::SelectComponent => 9,
        UiIcon::SelectVertex => 23,
        UiIcon::SelectEdge => 24,
        UiIcon::SelectFace => 25,
        UiIcon::SidebarModelingToolkit => 10,
        UiIcon::SidebarHumanIk => 11,
        UiIcon::SidebarAttributeEditor => 12,
        UiIcon::SidebarToolSettings => 13,
        UiIcon::SidebarChannelBox => 14,
        UiIcon::FileNew => 15,
        UiIcon::FileOpen => 16,
        UiIcon::FileSave => 17,
        UiIcon::PolyCube => 18,
        UiIcon::PolyPlane => 19,
        UiIcon::Undo => 20,
        UiIcon::Redo => 21,
        UiIcon::FrameSelected => 22,
    }
}
const ICON_COUNT: usize = 26;

struct Raster {
    w: usize,
    h: usize,
    /// Premultiplied RGBA in 0..=255, composited over transparent black.
    px: Vec<[f32; 4]>,
}

/// Mimics egui's disabled painter: fill alpha multiplied by `Visuals::disabled_alpha`.
fn dimmed(mut shapes: Vec<Shape>, opacity: f32) -> Vec<Shape> {
    for shape in &mut shapes {
        if let Shape::Path(path) = shape {
            path.fill = path.fill.gamma_multiply(opacity);
            if let ColorMode::Solid(c) = &mut path.stroke.color {
                *c = c.gamma_multiply(opacity);
            }
        }
    }
    shapes
}

fn rasterize(shapes: Vec<Shape>, canvas_pts: f32, ppp: f32) -> Raster {
    let mut tess = Tessellator::new(ppp, TessellationOptions::default(), [1, 1], vec![]);
    let mut prims = Vec::new();
    for shape in shapes {
        tess.tessellate_clipped_shape(
            ClippedShape {
                clip_rect: Rect::EVERYTHING,
                shape,
                transform_after_tessellation: egui::emath::TSTransform::IDENTITY,
            },
            &mut prims,
        );
    }
    let n = (canvas_pts * ppp).ceil() as usize;
    let mut px = vec![[0.0f32; 4]; n * n];
    for prim in prims {
        let Primitive::Mesh(mesh) = prim.primitive else {
            continue;
        };
        for tri in mesh.indices.as_chunks::<3>().0 {
            let v: Vec<_> = tri.iter().map(|&i| mesh.vertices[i as usize]).collect();
            let p: Vec<(f32, f32)> = v.iter().map(|v| (v.pos.x * ppp, v.pos.y * ppp)).collect();
            let area =
                (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-9 {
                continue;
            }
            // Half-open fill rule: a pixel centre on an edge belongs to the triangle
            // whose (orientation-normalised) edge points downwards, or rightwards if level.
            let owns = |w: f32, j: usize, k: usize| {
                if w > 1e-5 {
                    return true;
                }
                if w < -1e-5 {
                    return false;
                }
                let sign = area.signum();
                let (dx, dy) = (sign * (p[k].0 - p[j].0), sign * (p[k].1 - p[j].1));
                dy > 0.0 || (dy == 0.0 && dx > 0.0)
            };
            let lo = |a: f32, b: f32, c: f32| a.min(b).min(c).floor().max(0.0) as usize;
            let hi = |a: f32, b: f32, c: f32| (a.max(b).max(c).ceil().max(0.0) as usize + 1).min(n);
            for y in lo(p[0].1, p[1].1, p[2].1)..hi(p[0].1, p[1].1, p[2].1) {
                for x in lo(p[0].0, p[1].0, p[2].0)..hi(p[0].0, p[1].0, p[2].0) {
                    let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
                    let w0 = ((p[1].0 - cx) * (p[2].1 - cy) - (p[2].0 - cx) * (p[1].1 - cy)) / area;
                    let w1 = ((p[2].0 - cx) * (p[0].1 - cy) - (p[0].0 - cx) * (p[2].1 - cy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if !(owns(w0, 1, 2) && owns(w1, 2, 0) && owns(w2, 0, 1)) {
                        continue;
                    }
                    let mut src = [0.0f32; 4];
                    for (c, s) in src.iter_mut().enumerate() {
                        *s = w0 * f32::from(v[0].color.to_array()[c])
                            + w1 * f32::from(v[1].color.to_array()[c])
                            + w2 * f32::from(v[2].color.to_array()[c]);
                    }
                    let dst = &mut px[y * n + x];
                    let keep = 1.0 - src[3] / 255.0;
                    for c in 0..4 {
                        dst[c] = src[c] + dst[c] * keep;
                    }
                }
            }
        }
    }
    Raster { w: n, h: n, px }
}

struct Measure {
    coverage: f32,
    bbox_fraction: f32,
    outside_ink: f32,
    contrast: f32,
}

fn measure(icon: UiIcon, opacity: f32, size: f32, ppp: f32) -> Measure {
    let rect = Rect::from_min_size(Pos2::new(MARGIN, MARGIN), egui::vec2(size, size));
    let r = rasterize(
        dimmed(shapes(icon, rect), opacity),
        size + 2.0 * MARGIN,
        ppp,
    );
    // Feathering reaches half a physical px past an edge.
    let tol = 0.5 / ppp;
    let outer = rect.expand(tol);
    let (mut sum_a, mut outside, mut n_inside) = (0.0f32, 0.0f32, 0usize);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    let (mut contrast_sum, mut contrast_n) = (0.0f32, 0usize);
    for y in 0..r.h {
        for x in 0..r.w {
            let a = r.px[y * r.w + x][3] / 255.0;
            let c = Pos2::new((x as f32 + 0.5) / ppp, (y as f32 + 0.5) / ppp);
            if rect.contains(c) {
                n_inside += 1;
                sum_a += a;
            }
            if !outer.contains(c) && a > 0.01 {
                outside += a;
            }
            if a >= 0.5 * opacity && rect.contains(c) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
                let p = r.px[y * r.w + x];
                let lum =
                    |ch: &dyn Fn(usize) -> f32| 0.2126 * ch(0) + 0.7152 * ch(1) + 0.0722 * ch(2);
                let over = lum(&|i| p[i] + PANEL[i] * (1.0 - a));
                let bg = lum(&|i| PANEL[i]);
                contrast_sum += (over - bg).abs();
                contrast_n += 1;
            }
        }
    }
    let bbox = if x1 >= x0 && y1 >= y0 && x0 != usize::MAX {
        ((x1 - x0 + 1) * (y1 - y0 + 1)) as f32
    } else {
        0.0
    };
    Measure {
        coverage: sum_a / n_inside as f32,
        bbox_fraction: bbox / n_inside as f32,
        outside_ink: outside,
        contrast: contrast_sum / contrast_n.max(1) as f32,
    }
}

fn disabled_alpha() -> f32 {
    egui::Visuals::dark().disabled_alpha()
}

const SIZES: [f32; 3] = [16.0, 24.0, 32.0];
const SCALES: [f32; 2] = [1.0, 2.0];

#[test]
fn all_is_exhaustive_and_unique() {
    let mut seen = [false; ICON_COUNT];
    for &icon in ALL {
        let i = index(icon);
        assert!(!seen[i], "duplicate {icon:?}");
        seen[i] = true;
    }
    assert_eq!(ALL.len(), ICON_COUNT);
}

#[test]
fn ink_coverage_is_in_range() {
    for &icon in ALL {
        for opacity in [1.0, disabled_alpha()] {
            for size in SIZES {
                for ppp in SCALES {
                    let c = measure(icon, opacity, size, ppp).coverage;
                    assert!(
                        (0.08..=0.60).contains(&c),
                        "{icon:?} opacity {opacity} {size}px@{ppp}x coverage {c}"
                    );
                }
            }
        }
    }
}

#[test]
fn ink_fills_the_rect_and_stays_inside() {
    for &icon in ALL {
        for size in SIZES {
            for ppp in SCALES {
                let m = measure(icon, 1.0, size, ppp);
                assert!(
                    m.bbox_fraction >= 0.60,
                    "{icon:?} {size}px@{ppp}x bbox {}",
                    m.bbox_fraction
                );
                assert!(
                    m.outside_ink == 0.0,
                    "{icon:?} {size}px@{ppp}x ink outside rect: {}",
                    m.outside_ink
                );
            }
        }
    }
}

#[test]
fn disabled_is_dimmer_but_readable() {
    const READABLE_FLOOR: f32 = 40.0;
    for &icon in ALL {
        for size in SIZES {
            for ppp in SCALES {
                let n = measure(icon, 1.0, size, ppp).contrast;
                let d = measure(icon, disabled_alpha(), size, ppp).contrast;
                assert!(d < n, "{icon:?} {size}px@{ppp}x disabled {d} >= normal {n}");
                assert!(d > READABLE_FLOOR, "{icon:?} {size}px@{ppp}x disabled {d}");
            }
        }
    }
}

fn alpha_grid(icon: UiIcon, size: f32, ppp: f32) -> (Vec<f32>, usize) {
    alpha_grid_at(icon, 1.0, size, ppp)
}

/// One alpha grid per shape, each rasterized alone. Regions are tiled with gaps that
/// can legitimately dip below full alpha at small sizes, so seam and uniformity checks
/// look inside each region. Cross-region overlap is covered at 32px@2x by
/// `shapes_do_not_overlap` and at every size and scale by `regions_never_sum_above_full_coverage`.
fn region_grids(icon: UiIcon, opacity: f32, size: f32, ppp: f32) -> Vec<(Vec<f32>, usize)> {
    let rect = Rect::from_min_size(Pos2::new(MARGIN, MARGIN), egui::vec2(size, size));
    dimmed(shapes(icon, rect), opacity)
        .into_iter()
        .map(|shape| {
            let r = rasterize(vec![shape], size + 2.0 * MARGIN, ppp);
            (r.px.iter().map(|p| p[3] / 255.0).collect(), r.w)
        })
        .collect()
}

fn alpha_grid_at(icon: UiIcon, opacity: f32, size: f32, ppp: f32) -> (Vec<f32>, usize) {
    let rect = Rect::from_min_size(Pos2::new(MARGIN, MARGIN), egui::vec2(size, size));
    let r = rasterize(
        dimmed(shapes(icon, rect), opacity),
        size + 2.0 * MARGIN,
        ppp,
    );
    (r.px.iter().map(|p| p[3] / 255.0).collect(), r.w)
}

/// Floor for eroded interior pixels per icon at 32px@2x. The sparsest glyphs measure
/// 315 (LastUsed) and 330 (SelectComponent); 200 leaves slack for geometry tweaks but
/// fails if a glyph collapses to hairlines.
const MIN_ERODED_INTERIOR_PX: usize = 200;

#[test]
fn interior_is_solid_without_seams() {
    for &icon in ALL {
        for size in SIZES {
            for ppp in SCALES {
                let mut checked = 0;
                for (a, w) in region_grids(icon, 1.0, size, ppp) {
                    for y in 1..w - 1 {
                        for x in 1..w - 1 {
                            let eroded = (y - 1..=y + 1)
                                .all(|j| (x - 1..=x + 1).all(|i| a[j * w + i] >= 0.5));
                            checked += usize::from(eroded);
                            assert!(
                                !eroded || a[y * w + x] >= 0.98,
                                "{icon:?} {size}px@{ppp}x seam at ({x},{y}): alpha {}",
                                a[y * w + x]
                            );
                        }
                    }
                }
                // Thin strokes have no eroded interior at small sizes; the largest
                // configuration must have MIN_ERODED_INTERIOR_PX for every icon, so
                // the check is never vacuous.
                assert!(
                    size < 32.0 || ppp < 2.0 || checked >= MIN_ERODED_INTERIOR_PX,
                    "{icon:?} only {checked} interior pixels"
                );
            }
        }
    }
}

/// Intentional change detector: 12x12 occupancy at 24px@1x. Update on purpose.
fn golden(icon: UiIcon) -> [&'static str; 12] {
    match icon {
        UiIcon::Select => GOLDEN_SELECT,
        UiIcon::Lasso => GOLDEN_LASSO,
        UiIcon::PaintSelect => GOLDEN_PAINT_SELECT,
        UiIcon::Move => GOLDEN_MOVE,
        UiIcon::Rotate => GOLDEN_ROTATE,
        UiIcon::Scale => GOLDEN_SCALE,
        UiIcon::LastUsed => GOLDEN_LAST_USED,
        UiIcon::SelectHierarchy => GOLDEN_SELECT_HIERARCHY,
        UiIcon::SelectObject => GOLDEN_SELECT_OBJECT,
        UiIcon::SelectComponent => GOLDEN_SELECT_COMPONENT,
        UiIcon::SelectVertex => GOLDEN_SELECT_VERTEX,
        UiIcon::SelectEdge => GOLDEN_SELECT_EDGE,
        UiIcon::SelectFace => GOLDEN_SELECT_FACE,
        UiIcon::SidebarModelingToolkit => GOLDEN_SIDEBAR_MODELING_TOOLKIT,
        UiIcon::SidebarHumanIk => GOLDEN_SIDEBAR_HUMAN_IK,
        UiIcon::SidebarAttributeEditor => GOLDEN_SIDEBAR_ATTRIBUTE_EDITOR,
        UiIcon::SidebarToolSettings => GOLDEN_SIDEBAR_TOOL_SETTINGS,
        UiIcon::SidebarChannelBox => GOLDEN_SIDEBAR_CHANNEL_BOX,
        UiIcon::FileNew => GOLDEN_FILE_NEW,
        UiIcon::FileOpen => GOLDEN_FILE_OPEN,
        UiIcon::FileSave => GOLDEN_FILE_SAVE,
        UiIcon::PolyCube => GOLDEN_POLY_CUBE,
        UiIcon::PolyPlane => GOLDEN_POLY_PLANE,
        UiIcon::Undo => GOLDEN_UNDO,
        UiIcon::Redo => GOLDEN_REDO,
        UiIcon::FrameSelected => GOLDEN_FRAME_SELECTED,
    }
}

const GOLDEN_SELECT: [&str; 12] = [
    "............",
    ".#..........",
    ".###........",
    ".####.......",
    ".######.....",
    ".########...",
    ".#########..",
    ".#####......",
    ".#..##......",
    ".....#......",
    ".....##.....",
    "............",
];
const GOLDEN_LASSO: [&str; 12] = [
    "............",
    ".....#####..",
    "....##....#.",
    "...#.......#",
    "..#........#",
    "..#.......#.",
    "..#......##.",
    "..#.....##..",
    "..#..###....",
    ".#..........",
    "###.........",
    ".#..........",
];
const GOLDEN_PAINT_SELECT: [&str; 12] = [
    "............",
    ".........##.",
    "......#####.",
    "......####..",
    "......####..",
    ".....#####..",
    "...#.##.....",
    "..##........",
    "............",
    "............",
    ".#..........",
    "............",
];
const GOLDEN_MOVE: [&str; 12] = [
    "............",
    "............",
    "....##......",
    "...####.....",
    "............",
    "........#...",
    "........###.",
    "........###.",
    "..##....#...",
    ".###........",
    ".##.........",
    "............",
];
const GOLDEN_ROTATE: [&str; 12] = [
    ".....##.....",
    "...#...##...",
    "..#..###.#..",
    ".#........#.",
    "...######...",
    "#.#......#.#",
    "#.#......#.#",
    "...######...",
    ".#........#.",
    "..#.###..#..",
    "...##...#...",
    ".....##.....",
];
const GOLDEN_SCALE: [&str; 12] = [
    "............",
    "...####.....",
    "...####.....",
    "...####.....",
    "....##......",
    "........###.",
    "....##..####",
    "..#.##..####",
    ".###....###.",
    "#####.......",
    ".###........",
    "..#.........",
];
const GOLDEN_LAST_USED: [&str; 12] = [
    "............",
    ".......#.##.",
    "..........#.",
    "...........#",
    ".#....#...#.",
    ".#.....####.",
    ".#..........",
    ".#......#...",
    ".#......#...",
    ".#......#...",
    ".#.....#....",
    "............",
];

const GOLDEN_SELECT_HIERARCHY: [&str; 12] = [
    "............",
    ".....##.....",
    ".....##.....",
    "............",
    ".....##.....",
    "............",
    "..########..",
    "..#......#..",
    "..#......#..",
    ".###....###.",
    ".###....###.",
    "..#......#..",
];

const GOLDEN_SELECT_OBJECT: [&str; 12] = [
    "............",
    "....####....",
    "...######...",
    "..########..",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    "...######...",
    "....####....",
    "............",
];

const GOLDEN_SELECT_COMPONENT: [&str; 12] = [
    ".....##.....",
    ".....##.....",
    "...#....#...",
    ".#........#.",
    "...#....#...",
    ".#...##...#.",
    ".#...##...#.",
    "............",
    ".#........#.",
    "...#....#...",
    ".....##.....",
    ".....##.....",
];

const GOLDEN_SIDEBAR_MODELING_TOOLKIT: [&str; 12] = [
    "............",
    ".....##.....",
    "....####....",
    "....####....",
    ".....##.....",
    "............",
    ".#.########.",
    ".#...#.####.",
    "............",
    ".#.########.",
    "............",
    "............",
];
const GOLDEN_SIDEBAR_HUMAN_IK: [&str; 12] = [
    "............",
    ".....##.....",
    ".....##.....",
    ".....##.....",
    "..########..",
    ".....##.....",
    ".....##.....",
    "............",
    "....####....",
    "....####....",
    "....####....",
    "............",
];
const GOLDEN_SIDEBAR_ATTRIBUTE_EDITOR: [&str; 12] = [
    "............",
    "............",
    ".###.######.",
    ".....######.",
    "............",
    ".###.####...",
    ".###.####...",
    "............",
    ".....#####..",
    ".###.#####..",
    "............",
    "............",
];
const GOLDEN_SIDEBAR_TOOL_SETTINGS: [&str; 12] = [
    "............",
    "...#........",
    ".##########.",
    "...#........",
    "............",
    ".##########.",
    ".##########.",
    "............",
    "....##......",
    ".##########.",
    "....##......",
    "............",
];
const GOLDEN_SIDEBAR_CHANNEL_BOX: [&str; 12] = [
    "............",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    ".##########.",
    "............",
    ".##########.",
    ".##########.",
    "............",
];

const GOLDEN_FILE_NEW: [&str; 12] = [
    "............",
    ".#######....",
    ".########...",
    ".#########..",
    ".##########.",
    ".#########..",
    ".####...#...",
    ".####...#...",
    ".####.#####.",
    ".####.####..",
    ".####..##...",
    "............",
];
const GOLDEN_FILE_OPEN: [&str; 12] = [
    "............",
    "..###.......",
    ".####.......",
    ".####.......",
    ".##########.",
    ".##########.",
    "..#########.",
    "...########.",
    "..########..",
    "..########..",
    "..#######...",
    "............",
];
const GOLDEN_FILE_SAVE: [&str; 12] = [
    "............",
    ".....##.....",
    ".....##.....",
    ".....##.....",
    ".....##.....",
    "....####....",
    "..#..##..#..",
    ".##..##..##.",
    ".##......##.",
    ".##########.",
    "..########..",
    "............",
];

const GOLDEN_POLY_CUBE: [&str; 12] = [
    "............",
    ".....##.....",
    "...######...",
    "..########..",
    "..########..",
    "..########..",
    "..########..",
    "..########..",
    "...######...",
    ".....##.....",
    "..########..",
    "..########..",
];
const GOLDEN_POLY_PLANE: [&str; 12] = [
    "............",
    "............",
    "...########.",
    "...########.",
    "...#######..",
    "..########..",
    "..########..",
    "..#######...",
    ".########...",
    ".########...",
    "............",
    "............",
];
const GOLDEN_UNDO: [&str; 12] = [
    "............",
    "..#.#####...",
    ".##.######..",
    ".###.....##.",
    "..........##",
    "...........#",
    "...........#",
    "...........#",
    "..........##",
    ".........##.",
    ".........#..",
    "............",
];
const GOLDEN_REDO: [&str; 12] = [
    "............",
    "...#####.#..",
    "..######.##.",
    ".##.....###.",
    "##..........",
    "#...........",
    "#...........",
    "#...........",
    "##..........",
    ".##.........",
    "..#.........",
    "............",
];
const GOLDEN_FRAME_SELECTED: [&str; 12] = [
    "............",
    ".####..####.",
    ".#........#.",
    ".#........#.",
    ".#...##...#.",
    "....####....",
    "....####....",
    ".#...##...#.",
    ".#........#.",
    ".#........#.",
    ".####..####.",
    "............",
];
const GOLDEN_SELECT_VERTEX: [&str; 12] = [
    "........##..",
    "........##..",
    "###.....##..",
    "###.........",
    "............",
    "............",
    "............",
    "............",
    ".........###",
    "..##.....###",
    "..##........",
    "..##........",
];
const GOLDEN_SELECT_EDGE: [&str; 12] = [
    ".......##...",
    "..#####...#.",
    ".........##.",
    "#........##.",
    "#.........#.",
    ".#........#.",
    ".#........##",
    ".#........##",
    ".#........##",
    ".#..........",
    ".....#####..",
    "...##.......",
];
const GOLDEN_SELECT_FACE: [&str; 12] = [
    "............",
    "......###...",
    "..######.#..",
    ".#.##...##..",
    ".##.....##..",
    ".##..##..#..",
    "..#..##..##.",
    "..##.....##.",
    "..##...##.#.",
    "..#.######..",
    "...###......",
    "............",
];

fn occupancy_rows(icon: UiIcon) -> Vec<String> {
    let m = MARGIN as usize;
    let (a, w) = alpha_grid(icon, 24.0, 1.0);
    (0..12)
        .map(|cy| {
            (0..12)
                .map(|cx| {
                    let block: Vec<f32> = (0..2)
                        .flat_map(|j| (0..2).map(move |i| (j, i)))
                        .map(|(j, i)| a[(m + cy * 2 + j) * w + m + cx * 2 + i])
                        .collect();
                    if block.iter().sum::<f32>() / 4.0 >= 0.5 {
                        '#'
                    } else {
                        '.'
                    }
                })
                .collect()
        })
        .collect()
}

#[test]
fn occupancy_matches_golden() {
    for &icon in ALL {
        let rows = occupancy_rows(icon);
        assert_eq!(
            rows,
            golden(icon),
            "{icon:?} occupancy changed:\n{}",
            rows.join("\n")
        );
    }
}

/// Icons must stay tellable apart at the smallest size: at least 12% of the
/// 16x16 pixels differ between every pair.
#[test]
fn icons_are_pairwise_distinct_at_16px() {
    let masks: Vec<Vec<bool>> = ALL
        .iter()
        .map(|&icon| {
            let (a, w) = alpha_grid(icon, 16.0, 1.0);
            let m = MARGIN as usize;
            (0..16)
                .flat_map(|y| (0..16).map(move |x| (x, y)))
                .map(|(x, y)| a[(m + y) * w + m + x] >= 0.5)
                .collect()
        })
        .collect();
    for i in 0..ALL.len() {
        for j in i + 1..ALL.len() {
            let diff = masks[i]
                .iter()
                .zip(&masks[j])
                .filter(|(a, b)| a != b)
                .count();
            assert!(
                diff as f32 >= 0.12 * 256.0,
                "{:?} vs {:?}: only {diff} of 256 pixels differ",
                ALL[i],
                ALL[j]
            );
        }
    }
}

#[test]
fn uniform_interior_under_disabled_opacity() {
    for &icon in ALL {
        for size in SIZES {
            for ppp in SCALES {
                let mut any_interior = false;
                for (a, w) in region_grids(icon, disabled_alpha(), size, ppp) {
                    let mut interior = Vec::new();
                    for y in 1..w - 1 {
                        for x in 1..w - 1 {
                            let solid = (y - 1..=y + 1).all(|j| {
                                (x - 1..=x + 1).all(|i| a[j * w + i] >= 0.5 * disabled_alpha())
                            });
                            if solid {
                                interior.push((x, y, a[y * w + x]));
                            }
                        }
                    }
                    let mut sorted: Vec<f32> = interior.iter().map(|p| p.2).collect();
                    sorted.sort_by(f32::total_cmp);
                    any_interior |= !sorted.is_empty();
                    let Some(&median) = sorted.get(sorted.len() / 2) else {
                        continue;
                    };
                    for (x, y, v) in interior {
                        assert!(
                            (v - median).abs() <= 0.02,
                            "{icon:?} {size}px@{ppp}x alpha {v} at ({x},{y}) vs median {median}"
                        );
                    }
                }
                assert!(
                    size < 32.0 || ppp < 2.0 || any_interior,
                    "{icon:?} no interior"
                );
            }
        }
    }
}

#[test]
fn outlines_fan_fill_clockwise_without_fold_over() {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(24.0, 24.0));
    for &icon in ALL {
        for shape in shapes(icon, rect) {
            let Shape::Path(path) = shape else {
                panic!("{icon:?}: expected path shapes only");
            };
            if path.fill == Color32::TRANSPARENT {
                continue; // stroke-only path
            }
            let v = &path.points;
            for i in 2..v.len() {
                let (a, b) = (v[i - 1] - v[0], v[i] - v[0]);
                let area = 0.5 * (a.x * b.y - a.y * b.x);
                assert!(area > 0.0, "{icon:?} fan triangle {i} area {area}");
            }
        }
    }
}

#[test]
fn shared_edge_pixels_are_filled_once() {
    use eframe::egui::epaint::Mesh;
    let half = Color32::from_rgba_premultiplied(128, 128, 128, 128);
    let mut mesh = Mesh::default();
    for p in [(2.0, 2.0), (6.0, 2.0), (6.0, 6.0), (2.0, 6.0)] {
        mesh.colored_vertex(Pos2::new(p.0, p.1), half);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    let r = rasterize(vec![Shape::mesh(mesh)], 8.0, 1.0);
    let max = r.px.iter().map(|p| p[3] / 255.0).fold(0.0, f32::max);
    assert!(max <= 128.0 / 255.0 + 1e-3, "shared-edge pixel alpha {max}");
}

fn shape_color(shape: &Shape) -> Color32 {
    let Shape::Path(path) = shape else {
        panic!("expected path shapes only");
    };
    match path.stroke.color {
        ColorMode::Solid(c) if path.fill == Color32::TRANSPARENT => c,
        _ => path.fill,
    }
}

/// Alpha grid at 32px@2x of the listed shapes of `icon`, plus the canvas width in px.
fn shapes_alpha(shapes: Vec<Shape>) -> (Vec<f32>, usize) {
    let r = rasterize(shapes, 32.0 + 2.0 * MARGIN, 2.0);
    (r.px.iter().map(|p| p[3] / 255.0).collect(), r.w)
}

fn icon_shapes_32(icon: UiIcon) -> Vec<Shape> {
    let rect = Rect::from_min_size(Pos2::new(MARGIN, MARGIN), egui::vec2(32.0, 32.0));
    shapes(icon, rect)
}

struct Ink {
    /// Bounding box width and height in px.
    w: f32,
    h: f32,
    /// Ink centroid relative to the rect centre, in px; y grows downwards.
    cx: f32,
    cy: f32,
}

/// Ink (alpha >= 0.5) of every shape drawn in `color`, at 32px@2x.
fn ink_of_colour(icon: UiIcon, color: Color32) -> Ink {
    let picked: Vec<Shape> = icon_shapes_32(icon)
        .into_iter()
        .filter(|s| shape_color(s) == color)
        .collect();
    assert!(!picked.is_empty(), "{icon:?} has no {color:?} shape");
    let (a, w) = shapes_alpha(picked);
    let centre = (MARGIN + 16.0) * 2.0;
    let (mut n, mut sx, mut sy) = (0.0f32, 0.0f32, 0.0f32);
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (i, &v) in a.iter().enumerate() {
        if v < 0.5 {
            continue;
        }
        let (x, y) = (i % w, i / w);
        n += 1.0;
        sx += x as f32 + 0.5;
        sy += y as f32 + 0.5;
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    assert!(n > 0.0, "{icon:?} {color:?} has no ink");
    Ink {
        w: (x1 - x0 + 1) as f32,
        h: (y1 - y0 + 1) as f32,
        cx: sx / n - centre,
        cy: sy / n - centre,
    }
}

#[test]
fn axis_colours_match_axis_geometry() {
    for icon in [UiIcon::Move, UiIcon::Scale] {
        let x = ink_of_colour(icon, AXIS_X);
        assert!(x.w > x.h && x.cx > 0.0, "{icon:?} X not wide and right");
        let y = ink_of_colour(icon, AXIS_Y);
        assert!(y.h > y.w && y.cy < 0.0, "{icon:?} Y not tall and above");
        let z = ink_of_colour(icon, AXIS_Z);
        assert!(z.cx < 0.0 && z.cy > 0.0, "{icon:?} Z not lower-left");
    }
    let [x, y, z] = [AXIS_X, AXIS_Y, AXIS_Z].map(|c| ink_of_colour(UiIcon::Rotate, c));
    for other in [&x, &y] {
        assert!(z.w > other.w && z.h > other.h, "Rotate Z ring not largest");
        let roundness = |i: &Ink| i.w.min(i.h) / i.w.max(i.h);
        assert!(
            roundness(&z) > roundness(other),
            "Rotate Z ring not roundest"
        );
    }
    assert!(z.w.min(z.h) / z.w.max(z.h) > 0.9, "Rotate Z ring not round");
}

/// Per shape, the pixels (32px@2x) with alpha >= `min_alpha`.
fn shape_ink_masks(icon: UiIcon, min_alpha: f32) -> Vec<Vec<bool>> {
    icon_shapes_32(icon)
        .into_iter()
        .map(|s| {
            shapes_alpha(vec![s])
                .0
                .iter()
                .map(|&a| a >= min_alpha)
                .collect()
        })
        .collect()
}

/// Faint edge alpha, below the 0.5 ink threshold, so that strokes closer than their
/// own width still register as touching.
const OVERLAP_ALPHA: f32 = 0.25;

#[test]
fn shapes_do_not_overlap() {
    for &icon in ALL {
        let masks = shape_ink_masks(icon, OVERLAP_ALPHA);
        for i in 0..masks[0].len() {
            let n = masks.iter().filter(|m| m[i]).count();
            assert!(n < 2, "{icon:?} pixel {i} is inked by {n} shapes");
        }
    }
}

#[test]
fn every_shape_contributes_ink() {
    for &icon in ALL {
        for (k, m) in shape_ink_masks(icon, 0.5).iter().enumerate() {
            assert!(m.iter().any(|&b| b), "{icon:?} shape {k} has no ink");
        }
    }
}

/// Per pixel, the summed coverage of all shapes (each rasterized alone) may not exceed 1:
/// deliberate gaps leave it below 1, an overlap pushes it above at every size and scale.
#[test]
fn regions_never_sum_above_full_coverage() {
    // Calibrated to SIZES, SCALES and integer origins; at 1x with fractional origins the
    // REGION_GAP feathers overlap (up to ~1.09), so re-measure before widening the grid.
    const SUM_LIMIT: f32 = 1.02;
    for &icon in ALL {
        for size in SIZES {
            for ppp in SCALES {
                let grids = region_grids(icon, 1.0, size, ppp);
                for i in 0..grids[0].0.len() {
                    let sum: f32 = grids.iter().map(|(a, _)| a[i]).sum();
                    assert!(
                        sum <= SUM_LIMIT,
                        "{icon:?} {size}px@{ppp}x pixel {i} coverage sum {sum}"
                    );
                }
            }
        }
    }
}

/// Smallest empty space, in px, between the ink masks of two shapes: the centre distance of
/// their nearest pixels minus one pixel. Overlapping or edge-adjacent ink measures 0 or less.
fn mask_gap(a: &[bool], b: &[bool], w: usize) -> f32 {
    let points = |m: &[bool]| -> Vec<(f32, f32)> {
        (0..m.len())
            .filter(|&i| m[i])
            .map(|i| ((i % w) as f32, (i / w) as f32))
            .collect()
    };
    let (pa, pb) = (points(a), points(b));
    let mut best = f32::MAX;
    for &(ax, ay) in &pa {
        for &(bx, by) in &pb {
            best = best.min((ax - bx).hypot(ay - by));
        }
    }
    best - 1.0
}

/// Smallest gap between any two shapes of `icon` at 32px@2x, with the shape indices.
fn min_shape_gap(icon: UiIcon) -> (f32, usize, usize) {
    let masks = shape_ink_masks(icon, OVERLAP_ALPHA);
    let w = shapes_alpha(vec![]).1;
    let mut best = (f32::MAX, 0, 0);
    for i in 0..masks.len() {
        for j in i + 1..masks.len() {
            let gap = mask_gap(&masks[i], &masks[j], w);
            if gap < best.0 {
                best = (gap, i, j);
            }
        }
    }
    best
}

/// Neighbouring shapes need at least this much empty space between their ink masks.
const MIN_INK_GAP_PX: f32 = 1.0;

#[test]
fn regions_keep_a_visible_gap() {
    for &icon in ALL {
        let (gap, i, j) = min_shape_gap(icon);
        assert!(
            gap >= MIN_INK_GAP_PX,
            "{icon:?} shapes {i} and {j} are only {gap} px apart"
        );
    }
}

/// Index of the arrowhead among the Undo and Redo shapes.
const UNDO_HEAD_SHAPE: usize = 1;

/// Shape point lists of `icon` in a 24px rect at the origin, as plain coordinates.
fn icon_points(icon: UiIcon) -> Vec<Vec<Pos2>> {
    let rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(GRID, GRID));
    shapes(icon, rect)
        .into_iter()
        .map(|s| match s {
            Shape::Path(p) => p.points,
            _ => panic!("{icon:?}: expected path shapes only"),
        })
        .collect()
}

#[test]
fn redo_geometry_is_the_mirror_of_undo() {
    let (undo, redo) = (icon_points(UiIcon::Undo), icon_points(UiIcon::Redo));
    assert_eq!(undo.len(), redo.len());
    for (k, (u, r)) in undo.iter().zip(&redo).enumerate() {
        assert_eq!(u.len(), r.len());
        // The arc (shape 0) keeps its order; the arrowhead fan keeps vertex 0 and
        // reverses the rest to stay clockwise.
        let mut ordered = r.clone();
        if k == UNDO_HEAD_SHAPE {
            ordered[1..].reverse();
        }
        for (a, b) in u.iter().zip(&ordered) {
            assert!(
                (a.x + b.x - GRID).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4,
                "{a:?} is not the mirror of {b:?}"
            );
        }
    }
}

#[test]
fn redo_raster_is_undo_flipped_horizontally() {
    for (size, ppp) in [(16.0, 1.0), (24.0, 1.0), (32.0, 2.0)] {
        let (u, w) = alpha_grid(UiIcon::Undo, size, ppp);
        let (r, _) = alpha_grid(UiIcon::Redo, size, ppp);
        let worst = (0..w * w)
            .map(|i| (u[(i / w) * w + (w - 1 - i % w)] - r[i]).abs())
            .fold(0.0, f32::max);
        assert!(
            worst <= 1e-3,
            "{size}px@{ppp}x flipped Undo differs from Redo by {worst}"
        );
    }
}

/// Ink masks (alpha >= 0.5) of each shape of `icon` at `size`px@1x.
fn solid_masks(icon: UiIcon, size: f32) -> Vec<Vec<bool>> {
    let rect = Rect::from_min_size(Pos2::new(MARGIN, MARGIN), egui::vec2(size, size));
    shapes(icon, rect)
        .into_iter()
        .map(|s| {
            rasterize(vec![s], size + 2.0 * MARGIN, 1.0)
                .px
                .iter()
                .map(|p| p[3] / 255.0 >= 0.5)
                .collect()
        })
        .collect()
}

/// Most existing icons touch pixel to pixel at 24px@1x (their gaps are under a pixel), so
/// only the History arrow, whose head once fused with its arc, is held to a full pixel of
/// clear space at the two smallest sizes.
#[test]
fn history_head_is_a_pixel_clear_of_the_arc_at_small_sizes() {
    for icon in [UiIcon::Undo, UiIcon::Redo] {
        for size in [16.0, 24.0] {
            let m = solid_masks(icon, size);
            let w = (size + 2.0 * MARGIN) as usize;
            let gap = mask_gap(&m[0], &m[UNDO_HEAD_SHAPE], w);
            assert!(
                gap >= MIN_INK_GAP_PX,
                "{icon:?} {size}px arc-head gap {gap}"
            );
        }
    }
}
