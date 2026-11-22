#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use glam::{Vec2, Vec3};

use super::{PlateQuad, plate_quad};
use crate::image_plane::model::{Coverage, ImagePlane, ImagePlaneEdit, PlateFit};
use crate::viewport::{Camera, FilmBack, film_gate_ndc};

/// Gate width and height in inches (the Create Camera defaults) and the aspect at which the
/// Fill gate is exactly the view, so the gate corners are (+-1, +-1).
pub(super) const W: f64 = 1.4173;
pub(super) const H: f64 = 0.9449;
pub(super) const GATE_ASPECT: f32 = (1.4173 / 0.9449) as f32;
const TOL: f32 = 1e-5;

pub(super) fn camera() -> Camera {
    Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 1.0, 0.1, 10000.0).unwrap()
}

pub(super) fn plane(size: (u32, u32), edit: impl FnOnce(&mut ImagePlaneEdit)) -> ImagePlane {
    let mut e = ImagePlaneEdit {
        image_name: Some("p.png".into()),
        ..ImagePlaneEdit::default()
    };
    edit(&mut e);
    ImagePlane::create(&e, size).unwrap()
}

pub(super) fn fitted(size: (u32, u32), fit: PlateFit) -> ImagePlane {
    plane(size, |e| e.fit = Some(fit))
}

pub(super) fn quad(p: &ImagePlane, size: (u32, u32), aspect: f32) -> Option<PlateQuad> {
    plate_quad(p, size, &FilmBack::DEFAULT, aspect, &camera())
}

pub(super) fn close(actual: Vec2, expected: (f64, f64), what: &str) {
    assert!(
        (f64::from(actual.x) - expected.0).abs() <= f64::from(TOL)
            && (f64::from(actual.y) - expected.1).abs() <= f64::from(TOL),
        "{what}: {actual:?} vs {expected:?}"
    );
}

pub(super) fn corners(q: &PlateQuad, sx: f64, sy: f64, what: &str) {
    // Lower-left, lower-right, upper-right, upper-left.
    for (c, e) in q
        .ndc
        .iter()
        .zip([(-sx, -sy), (sx, -sy), (sx, sy), (-sx, sy)])
    {
        close(*c, e, what);
    }
}

/// One hand-worked case: the fit, the quad's (sx, sy) in the gate frame and the fraction of the
/// region's u and v range the texture keeps (the rest is cropped).
type Case = (PlateFit, (f64, f64), (f64, f64));

/// The uv box of a whole-image plate that keeps `(kx, ky)` of each axis, centred: the lower-left
/// corner is (0.5 - kx / 2, 0.5 + ky / 2) and the upper-right (0.5 + kx / 2, 0.5 - ky / 2).
fn centred_uv(q: &PlateQuad, kx: f64, ky: f64, what: &str) {
    close(q.uv[0], (0.5 - kx / 2.0, 0.5 + ky / 2.0), what);
    close(q.uv[1], (0.5 + kx / 2.0, 0.5 + ky / 2.0), what);
    close(q.uv[2], (0.5 + kx / 2.0, 0.5 - ky / 2.0), what);
    close(q.uv[3], (0.5 - kx / 2.0, 0.5 - ky / 2.0), what);
}

#[test]
fn every_fit_at_an_image_aspect_below_at_and_above_the_gate_aspect() {
    let g = W / H; // 1.49994: the gate is wider than tall
    let a = 1920.0 / 1080.0;
    // Raw plate-to-gate ratios (rx, ry) are Best min-scale, Fill max-scale, Horizontal width,
    // Vertical height; a ratio above 1 is cropped, so the quad is min(r, 1) and the texture keeps
    // min(r, 1) / r of that axis. The gate is the square (+-1, +-1) at this aspect.
    let cases: [((u32, u32), [Case; 5]); 3] = [
        // Square image, aspect 1 < g: Best fits the height; Fill and Horizontal overflow in v.
        (
            (1000, 1000),
            [
                (PlateFit::Best, (1.0 / g, 1.0), (1.0, 1.0)),
                (PlateFit::Fill, (1.0, 1.0), (1.0, 1.0 / g)),
                (PlateFit::Horizontal, (1.0, 1.0), (1.0, 1.0 / g)),
                (PlateFit::Vertical, (1.0 / g, 1.0), (1.0, 1.0)),
                (PlateFit::ToSize, (1.0, 1.0), (1.0, 1.0)),
            ],
        ),
        // 16:9, aspect 1.78 > g: Best fits the width; Fill and Vertical overflow in u.
        (
            (1920, 1080),
            [
                (PlateFit::Best, (1.0, g / a), (1.0, 1.0)),
                (PlateFit::Fill, (1.0, 1.0), (g / a, 1.0)),
                (PlateFit::Horizontal, (1.0, g / a), (1.0, 1.0)),
                (PlateFit::Vertical, (1.0, 1.0), (g / a, 1.0)),
                (PlateFit::ToSize, (1.0, 1.0), (1.0, 1.0)),
            ],
        ),
        // Exactly the gate's aspect: every fit is the gate itself with nothing cropped.
        (
            (14173, 9449),
            [
                (PlateFit::Best, (1.0, 1.0), (1.0, 1.0)),
                (PlateFit::Fill, (1.0, 1.0), (1.0, 1.0)),
                (PlateFit::Horizontal, (1.0, 1.0), (1.0, 1.0)),
                (PlateFit::Vertical, (1.0, 1.0), (1.0, 1.0)),
                (PlateFit::ToSize, (1.0, 1.0), (1.0, 1.0)),
            ],
        ),
    ];
    for (size, fits) in cases {
        for (fit, (sx, sy), (kx, ky)) in fits {
            let q = quad(&fitted(size, fit), size, GATE_ASPECT).unwrap();
            let what = format!("{size:?} {fit:?}");
            corners(&q, sx, sy, &what);
            centred_uv(&q, kx, ky, &what);
        }
    }
}

/// The (a, b) coordinates of an NDC point in the gate's own frame (the gate corners give the
/// centre C and half edges U, V; the point is C + a U + b V), solved with Cramer's rule.
fn gate_frame(film: &FilmBack, aspect: f32, p: Vec2) -> (f64, f64) {
    let g = film_gate_ndc(film, aspect).map(|c| (f64::from(c.x), f64::from(c.y)));
    let c = (
        (g[0].0 + g[1].0 + g[2].0 + g[3].0) / 4.0,
        (g[0].1 + g[1].1 + g[2].1 + g[3].1) / 4.0,
    );
    let u = ((g[1].0 - g[0].0) / 2.0, (g[1].1 - g[0].1) / 2.0);
    let v = ((g[3].0 - g[0].0) / 2.0, (g[3].1 - g[0].1) / 2.0);
    let d = (f64::from(p.x) - c.0, f64::from(p.y) - c.1);
    let det = u.0 * v.1 - u.1 * v.0;
    ((d.0 * v.1 - d.1 * v.0) / det, (u.0 * d.1 - u.1 * d.0) / det)
}

#[test]
fn no_plate_corner_leaves_the_film_gate_under_offset_overscan_roll_and_squeeze() {
    let mut film = FilmBack::DEFAULT;
    film.film_offset_in = [0.3, 0.1];
    film.film_roll_degrees = 10.0;
    film.lens_squeeze = 1.5;
    film.overscan = 1.5;
    for size in [(1000, 1000), (1920, 1080), (400, 3000), (14173, 9449)] {
        for fit in [
            PlateFit::Best,
            PlateFit::Fill,
            PlateFit::Horizontal,
            PlateFit::Vertical,
            PlateFit::ToSize,
        ] {
            for aspect in [0.7_f32, 1.5, 2.4] {
                let q = plate_quad(&fitted(size, fit), size, &film, aspect, &camera()).unwrap();
                for c in q.ndc {
                    let (a, b) = gate_frame(&film, aspect, c);
                    assert!(
                        a.abs() <= 1.0 + 1e-5 && b.abs() <= 1.0 + 1e-5,
                        "{size:?} {fit:?} {aspect}: ({a}, {b})"
                    );
                }
            }
        }
    }
}

#[test]
fn a_cropped_plate_keeps_the_texture_range_centred_on_an_off_centre_region_under_roll() {
    // Region 640 x 360 at (320, 180) of 1920 x 1080: u 1/6..1/2 (centre 1/3, half 1/6) and, v
    // down, 5/6 (bottom) .. 1/2 (top) (centre 2/3, half 1/6). The region is 16:9 > g, so Fill
    // overflows in u and keeps g / A of it; Horizontal keeps everything.
    let g = W / H;
    let a = 16.0 / 9.0;
    let size = (1920, 1080);
    let region = Coverage {
        origin_x: 320,
        origin_y: 180,
        width: 640,
        height: 360,
    };
    let mut film = FilmBack::DEFAULT;
    film.film_offset_in = [0.2, 0.1];
    film.film_roll_degrees = 25.0;
    let fill = plate_quad(
        &covered(size, region, PlateFit::Fill),
        size,
        &film,
        GATE_ASPECT,
        &camera(),
    )
    .unwrap();
    let half = (1.0 / 6.0) * (g / a);
    close(fill.uv[0], (1.0 / 3.0 - half, 5.0 / 6.0), "fill ll");
    close(fill.uv[2], (1.0 / 3.0 + half, 0.5), "fill ur");
    let horizontal = plate_quad(
        &covered(size, region, PlateFit::Horizontal),
        size,
        &film,
        GATE_ASPECT,
        &camera(),
    )
    .unwrap();
    close(horizontal.uv[0], (1.0 / 6.0, 5.0 / 6.0), "horizontal ll");
    close(horizontal.uv[2], (0.5, 0.5), "horizontal ur");
    // The crop lives in the plane's own frame: the rolled quad is still the rolled gate.
    for c in fill.ndc {
        let (aa, bb) = gate_frame(&film, GATE_ASPECT, c);
        assert!(
            aa.abs() <= 1.0 + 1e-5 && bb.abs() <= 1.0 + 1e-5,
            "{aa} {bb}"
        );
    }
}

#[test]
fn a_filled_plate_under_a_ninety_degree_roll_crops_v_and_fills_the_rolled_gate() {
    // film_tests: Horizontal fit, roll 90 at aspect 1: gate corners (k,-1), (k,1), (-k,1),
    // (-k,-1), k = H / W. A square image under Fill has raw ratios (1, 1 / k): the quad is
    // the whole gate and the texture keeps k of v: v 0.5 -+ k / 2.
    let mut film = FilmBack::DEFAULT;
    film.film_fit = crate::viewport::FilmFit::Horizontal;
    film.film_roll_degrees = 90.0;
    let size = (1000, 1000);
    let q = plate_quad(&fitted(size, PlateFit::Fill), size, &film, 1.0, &camera()).unwrap();
    let k = H / W;
    for (c, e) in q
        .ndc
        .iter()
        .zip([(k, -1.0), (k, 1.0), (-k, 1.0), (-k, -1.0)])
    {
        close(*c, e, "rolled fill");
    }
    centred_uv(&q, 1.0, k, "rolled fill uv");
}

#[test]
fn the_plate_follows_the_viewport_aspect_like_the_gate_does() {
    // Aspect 1: the Fill gate is (+-g, +-1) (film_tests). A 16:9 Best plate keeps the gate's
    // width and has the height ratio g / A.
    let g = W / H;
    let a = 1920.0 / 1080.0;
    let q = quad(&fitted((1920, 1080), PlateFit::Best), (1920, 1080), 1.0).unwrap();
    corners(&q, g, g / a, "aspect 1");
    // Aspect 3: the gate is (+-1, +-3 * H / W).
    let tall = 3.0 * H / W;
    let q = quad(&fitted((1920, 1080), PlateFit::Best), (1920, 1080), 3.0).unwrap();
    corners(&q, 1.0, tall * g / a, "aspect 3");
}

#[test]
fn a_to_size_plate_is_exactly_the_film_gate_for_offset_roll_squeeze_and_overscan() {
    let mut film = FilmBack::DEFAULT;
    film.film_offset_in = [0.3, 0.1];
    film.film_roll_degrees = 10.0;
    film.lens_squeeze = 1.5;
    film.overscan = 1.5;
    let size = (1920, 1080);
    let p = fitted(size, PlateFit::ToSize);
    for aspect in [0.8_f32, 1.5, 2.4] {
        let q = plate_quad(&p, size, &film, aspect, &camera()).unwrap();
        let gate = film_gate_ndc(&film, aspect);
        for (c, e) in q.ndc.iter().zip(gate) {
            assert!(
                (*c - e).abs().max_element() <= 1e-6,
                "{aspect}: {c:?} vs {e:?}"
            );
        }
    }
}

#[test]
fn the_corner_order_is_lower_left_lower_right_upper_right_upper_left_for_ndc_and_uv() {
    let size = (1920, 1080);
    let q = quad(&fitted(size, PlateFit::ToSize), size, GATE_ASPECT).unwrap();
    assert!(q.ndc[0].x < q.ndc[1].x && q.ndc[0].y < q.ndc[3].y);
    assert!(q.ndc[1].y < q.ndc[2].y && q.ndc[3].x < q.ndc[2].x);
    // Texture coordinates, v down: the lower-left corner shows the bottom row (v = 1).
    close(q.uv[0], (0.0, 1.0), "uv ll");
    close(q.uv[1], (1.0, 1.0), "uv lr");
    close(q.uv[2], (1.0, 0.0), "uv ur");
    close(q.uv[3], (0.0, 0.0), "uv ul");
}

pub(super) fn covered(size: (u32, u32), c: Coverage, fit: PlateFit) -> ImagePlane {
    plane(size, |e| {
        e.coverage = Some(Some(c));
        e.fit = Some(fit);
    })
}

#[test]
fn coverage_crops_the_texture_coordinates_on_the_right_axes() {
    let size = (1920, 1080);
    let cov = |ox, oy, w, h| Coverage {
        origin_x: ox,
        origin_y: oy,
        width: w,
        height: h,
    };
    // A 960 x 540 quarter at the lower left: u 0..0.5, and v (down) from 1 up to 0.5.
    let q = quad(
        &covered(size, cov(0, 0, 960, 540), PlateFit::ToSize),
        size,
        GATE_ASPECT,
    )
    .unwrap();
    close(q.uv[0], (0.0, 1.0), "quarter ll");
    close(q.uv[2], (0.5, 0.5), "quarter ur");
    // A centred 960 x 540: origin (480, 270): u 0.25..0.75, v 0.75..0.25.
    let q = quad(
        &covered(size, cov(480, 270, 960, 540), PlateFit::ToSize),
        size,
        GATE_ASPECT,
    )
    .unwrap();
    close(q.uv[0], (0.25, 0.75), "centre ll");
    close(q.uv[1], (0.75, 0.75), "centre lr");
    close(q.uv[2], (0.75, 0.25), "centre ur");
    close(q.uv[3], (0.25, 0.25), "centre ul");
    // A tall strip 480 x 1080 keeps u and v apart: u 0..0.25, v 1..0.
    let q = quad(
        &covered(size, cov(0, 0, 480, 1080), PlateFit::ToSize),
        size,
        GATE_ASPECT,
    )
    .unwrap();
    close(q.uv[2], (0.25, 0.0), "strip ur");
    close(q.uv[0], (0.0, 1.0), "strip ll");
    // One pixel at the upper right: u 1919/1920..1, v 1/1080..0 (counted from the lower left).
    let q = quad(
        &covered(size, cov(1919, 1079, 1, 1), PlateFit::ToSize),
        size,
        GATE_ASPECT,
    )
    .unwrap();
    close(q.uv[0], (1919.0 / 1920.0, 1.0 / 1080.0), "pixel ll");
    close(q.uv[2], (1.0, 0.0), "pixel ur");
    // The full image given as a coverage is the same as none.
    let full = quad(
        &covered(size, cov(0, 0, 1920, 1080), PlateFit::Best),
        size,
        GATE_ASPECT,
    )
    .unwrap();
    let none = quad(&fitted(size, PlateFit::Best), size, GATE_ASPECT).unwrap();
    assert_eq!(full, none);
}
