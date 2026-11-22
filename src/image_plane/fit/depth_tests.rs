#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use glam::Vec3;

use super::plate_quad;
use super::tests::{GATE_ASPECT, H, W, camera, close, corners, covered, fitted, plane, quad};
use crate::image_plane::model::{Coverage, PlateDisplayMode, PlateFit};
use crate::viewport::{Camera, FilmBack, OrthographicView};

#[test]
fn the_clip_depth_is_zero_at_near_one_at_far_and_the_hand_value_between() {
    let size = (4, 2);
    let at = |d: f32| {
        let p = plane(size, |e| e.depth = Some(d));
        quad(&p, size, GATE_ASPECT).map(|q| q.z_ndc)
    };
    // n = 0.1, f = 10000, d = 100: f (d - n) / (d (f - n)) = 10000 * 99.9 / (100 * 9999.9).
    let hand = 999_000.0 / 999_990.0;
    assert!((f64::from(at(100.0).unwrap()) - hand).abs() < 1e-7);
    assert_eq!(at(0.1), Some(0.0));
    assert_eq!(at(10000.0), Some(1.0));
    // Outside near..far is clipped by the hardware: nothing to draw.
    assert_eq!(at(0.05), None);
    assert_eq!(at(10000.5), None);
    assert_eq!(at(f32::MAX), None);
    // The clip planes are read from the camera on every call.
    let p = plane(size, |e| e.depth = Some(100.0));
    let wide = Camera::new(Vec3::ZERO, 0.0, 0.0, 5.0, 1.0, 1.0, 1000.0).unwrap();
    let z = plate_quad(&p, size, &FilmBack::DEFAULT, GATE_ASPECT, &wide)
        .unwrap()
        .z_ndc;
    // n = 1, f = 1000, d = 100: 1000 * 99 / (100 * 999).
    assert!((f64::from(z) - 99_000.0 / 99_900.0).abs() < 1e-7);
}

#[test]
fn nothing_is_drawn_for_display_none_an_orthographic_camera_or_degenerate_input() {
    let size = (1920, 1080);
    let p = fitted(size, PlateFit::Best);
    let mut none = p.clone();
    none.display_mode = PlateDisplayMode::None;
    assert!(quad(&none, size, GATE_ASPECT).is_none());
    // Outline still has a quad: the renderer draws only its edges.
    let mut outline = p.clone();
    outline.display_mode = PlateDisplayMode::Outline;
    assert!(quad(&outline, size, GATE_ASPECT).is_some());
    let mut top = camera();
    top.show_orthographic(OrthographicView::Top).unwrap();
    assert!(plate_quad(&p, size, &FilmBack::DEFAULT, GATE_ASPECT, &top).is_none());
    for bad in [(0, 1080), (1920, 0), (0, 0)] {
        assert!(quad(&p, bad, GATE_ASPECT).is_none(), "{bad:?}");
    }
    for aspect in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert!(quad(&p, size, aspect).is_none(), "{aspect}");
    }
    let mut nan_depth = p.clone();
    nan_depth.depth = f32::NAN;
    assert!(quad(&nan_depth, size, GATE_ASPECT).is_none());
    // A coverage the image cannot hold (the model refuses it; the geometry must not panic).
    for c in [
        Coverage {
            origin_x: 1,
            origin_y: 0,
            width: 1920,
            height: 1080,
        },
        Coverage {
            origin_x: 0,
            origin_y: 1,
            width: 1920,
            height: 1080,
        },
        Coverage {
            origin_x: u32::MAX,
            origin_y: 0,
            width: 2,
            height: 1,
        },
        Coverage {
            origin_x: 0,
            origin_y: 0,
            width: 0,
            height: 1,
        },
        Coverage {
            origin_x: 0,
            origin_y: 0,
            width: 1,
            height: 0,
        },
    ] {
        let mut bad = p.clone();
        bad.coverage = Some(c);
        assert!(quad(&bad, size, GATE_ASPECT).is_none(), "{c:?}");
    }
    let mut film = FilmBack::DEFAULT;
    film.aperture_in[0] = 0.0;
    assert!(plate_quad(&p, size, &film, GATE_ASPECT, &camera()).is_none());
    film = FilmBack::DEFAULT;
    film.lens_squeeze = f64::NAN;
    assert!(plate_quad(&p, size, &film, GATE_ASPECT, &camera()).is_none());
}

#[test]
fn the_squeezed_aperture_sets_the_gate_width_the_fit_works_against() {
    // film_tests: lens squeeze 2 at aspect 2 * gate aspect makes the gate (+-1, +-1) with the
    // gate 2 W wide, so its aspect is 2 g = 3.0. A 16:9 image is narrower, so Best fits the
    // height: sy = 1 and sx = A / (2 g).
    let mut film = FilmBack::DEFAULT;
    film.lens_squeeze = 2.0;
    let size = (1920, 1080);
    let q = plate_quad(
        &fitted(size, PlateFit::Best),
        size,
        &film,
        2.0 * GATE_ASPECT,
        &camera(),
    )
    .unwrap();
    corners(&q, (1920.0 / 1080.0) / (2.0 * W / H), 1.0, "squeezed");
}

#[test]
fn the_images_own_pixel_aspect_is_not_applied() {
    // PROVISIONAL (squeezeCorrection is out of scope): the quad depends on the pixel counts'
    // ratio only, so the same ratio at any size is the same plate, and an anamorphic frame is
    // fitted as its stored width and height say.
    let small = fitted((1920, 1080), PlateFit::Best);
    let big = fitted((3840, 2160), PlateFit::Best);
    assert_eq!(
        quad(&small, (1920, 1080), GATE_ASPECT).unwrap().ndc,
        quad(&big, (3840, 2160), GATE_ASPECT).unwrap().ndc
    );
}

#[test]
fn a_best_plate_under_a_ninety_degree_roll_is_the_rolled_gate_shrunk_by_hand() {
    // film_tests: Horizontal fit, roll 90 at aspect 1: the gate corners are
    // (k,-1), (k,1), (-k,1), (-k,-1) with k = H / W. A square image under Best has the ratios
    // (sx, sy) = (k, 1), and C + a*sx*U + b*sy*V with U = (0, 1), V = (-k, 0) gives
    // (k,-k), (k,k), (-k,k), (-k,-k).
    let mut film = FilmBack::DEFAULT;
    film.film_fit = crate::viewport::FilmFit::Horizontal;
    film.film_roll_degrees = 90.0;
    let size = (1000, 1000);
    let q = plate_quad(&fitted(size, PlateFit::Best), size, &film, 1.0, &camera()).unwrap();
    let k = H / W;
    for (c, e) in q.ndc.iter().zip([(k, -k), (k, k), (-k, k), (-k, -k)]) {
        close(*c, e, "rolled");
    }
}

#[test]
fn a_fit_offset_moves_the_plate_with_the_gate() {
    // film_tests: Horizontal fit, Film Fit Offset 0.2 in moves the gate by
    // -ffo * 25.4 / 35 / ty in y, ty = vfa * 25.4 / 70 at this aspect. A plate with the gate's
    // own aspect fits it exactly, so it moves with it (and not about the screen centre).
    let mut film = FilmBack::DEFAULT;
    film.film_fit = crate::viewport::FilmFit::Horizontal;
    film.film_fit_offset_in = 0.2;
    let shift = 0.2 * 25.4 / 35.0 / (H * 25.4 / 70.0);
    let size = (14173, 9449);
    let q = plate_quad(
        &fitted(size, PlateFit::Best),
        size,
        &film,
        GATE_ASPECT,
        &camera(),
    )
    .unwrap();
    for (c, e) in q.ndc.iter().zip([
        (-1.0, -1.0 - shift),
        (1.0, -1.0 - shift),
        (1.0, 1.0 - shift),
        (-1.0, 1.0 - shift),
    ]) {
        close(*c, e, "fit offset");
    }
}

#[test]
fn the_fit_uses_the_covered_regions_aspect() {
    // A 540 x 540 square cut from a 16:9 image fits like a square image: Best is (1 / g, 1).
    let size = (1920, 1080);
    let c = Coverage {
        origin_x: 100,
        origin_y: 100,
        width: 540,
        height: 540,
    };
    let q = quad(&covered(size, c, PlateFit::Best), size, GATE_ASPECT).unwrap();
    corners(&q, H / W, 1.0, "square region");
}
