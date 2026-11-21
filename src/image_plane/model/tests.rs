#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::PathBuf;

use super::{
    Coverage, ImagePlane, ImagePlaneEdit, ImagePlaneError, PlateChange, PlateDisplayMode, PlateFit,
};

const SIZE: (u32, u32) = (1920, 1080);

fn named() -> ImagePlaneEdit {
    ImagePlaneEdit {
        image_name: Some(PathBuf::from("plate.0001.png")),
        ..ImagePlaneEdit::default()
    }
}

fn plane() -> ImagePlane {
    ImagePlane::create(&named(), SIZE).unwrap()
}

fn with(f: impl FnOnce(&mut ImagePlaneEdit)) -> ImagePlaneEdit {
    let mut e = ImagePlaneEdit::default();
    f(&mut e);
    e
}

fn field(err: ImagePlaneError) -> &'static str {
    match err {
        ImagePlaneError::NotFinite { field } | ImagePlaneError::OutOfRange { field, .. } => field,
        ImagePlaneError::DepthOutOfRange => "depth",
        other => panic!("not a field error: {other:?}"),
    }
}

#[test]
fn defaults_are_the_node_page_values() {
    let p = plane();
    assert_eq!(p.image_name, PathBuf::from("plate.0001.png"));
    assert!(!p.use_frame_extension);
    assert_eq!(p.frame_offset, 0);
    assert_eq!(p.frame_cache, 24);
    assert_eq!(p.fit, PlateFit::Best);
    assert_eq!(p.depth, 100.0);
    assert_eq!(p.alpha_gain, 1.0);
    assert_eq!(p.display_mode, PlateDisplayMode::Rgba);
    assert_eq!(p.coverage, None);
}

#[test]
fn create_requires_a_non_empty_image_name() {
    assert_eq!(
        ImagePlane::create(&ImagePlaneEdit::default(), SIZE),
        Err(ImagePlaneError::ImageNameMissing)
    );
    let empty = with(|e| e.image_name = Some(PathBuf::new()));
    assert_eq!(
        ImagePlane::create(&empty, SIZE),
        Err(ImagePlaneError::ImageNameEmpty)
    );
    let mut p = plane();
    assert_eq!(p.apply(&empty, SIZE), Err(ImagePlaneError::ImageNameEmpty));
    assert_eq!(p, plane());
}

#[test]
fn create_applies_the_edit_over_the_defaults() {
    let mut e = named();
    e.depth = Some(250.0);
    e.fit = Some(PlateFit::ToSize);
    e.use_frame_extension = Some(true);
    let p = ImagePlane::create(&e, SIZE).unwrap();
    assert_eq!(p.depth, 250.0);
    assert_eq!(p.fit, PlateFit::ToSize);
    assert!(p.use_frame_extension);
    assert_eq!(p.frame_cache, 24);
}

#[test]
fn depth_accepts_the_f32_maximum_and_refuses_zero_negative_and_non_finite() {
    for ok in [f32::MIN_POSITIVE, 1e-30, f32::MAX] {
        let mut p = plane();
        assert_eq!(
            p.apply(&with(|e| e.depth = Some(ok)), SIZE),
            Ok(PlateChange::Changed)
        );
        assert_eq!(p.depth, ok);
    }
    for bad in [0.0, -0.0, -1.0, f32::NEG_INFINITY] {
        let mut p = plane();
        let err = p.apply(&with(|e| e.depth = Some(bad)), SIZE).unwrap_err();
        assert_eq!(field(err), "depth", "{bad}");
        assert_eq!(p, plane());
    }
    for bad in [f32::NAN, f32::INFINITY] {
        let mut p = plane();
        let err = p.apply(&with(|e| e.depth = Some(bad)), SIZE).unwrap_err();
        assert_eq!(err, ImagePlaneError::NotFinite { field: "depth" });
        assert_eq!(p, plane());
    }
}

#[test]
fn alpha_gain_accepts_zero_and_1000_and_refuses_beyond() {
    for ok in [0.0, 0.5, 1000.0] {
        let mut p = plane();
        p.apply(&with(|e| e.alpha_gain = Some(ok)), SIZE).unwrap();
        assert_eq!(p.alpha_gain, ok);
    }
    for bad in [-f32::MIN_POSITIVE, 1000.0001, f32::MAX] {
        let mut p = plane();
        let err = p
            .apply(&with(|e| e.alpha_gain = Some(bad)), SIZE)
            .unwrap_err();
        assert_eq!(field(err), "alpha_gain", "{bad}");
        assert_eq!(p, plane());
    }
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut p = plane();
        let err = p
            .apply(&with(|e| e.alpha_gain = Some(bad)), SIZE)
            .unwrap_err();
        assert_eq!(
            err,
            ImagePlaneError::NotFinite {
                field: "alpha_gain"
            }
        );
    }
}

#[test]
fn frame_offset_accepts_plus_minus_a_million_and_refuses_one_beyond() {
    for ok in [-1_000_000, 0, 1_000_000] {
        let mut p = plane();
        p.apply(&with(|e| e.frame_offset = Some(ok)), SIZE).unwrap();
        assert_eq!(p.frame_offset, ok);
    }
    for bad in [-1_000_001, 1_000_001, i32::MIN, i32::MAX] {
        let mut p = plane();
        let err = p
            .apply(&with(|e| e.frame_offset = Some(bad)), SIZE)
            .unwrap_err();
        assert_eq!(field(err), "frame_offset", "{bad}");
        assert_eq!(p, plane());
    }
}

#[test]
fn frame_cache_accepts_1_to_1000_and_refuses_0_and_1001() {
    for ok in [1, 24, 1000] {
        let mut p = plane();
        p.apply(&with(|e| e.frame_cache = Some(ok)), SIZE).unwrap();
        assert_eq!(p.frame_cache, ok);
    }
    for bad in [0, 1001, u32::MAX] {
        let mut p = plane();
        let err = p
            .apply(&with(|e| e.frame_cache = Some(bad)), SIZE)
            .unwrap_err();
        assert_eq!(field(err), "frame_cache", "{bad}");
        assert_eq!(p, plane());
    }
}

fn cov(origin_x: u32, origin_y: u32, width: u32, height: u32) -> ImagePlaneEdit {
    with(|e| {
        e.coverage = Some(Some(Coverage {
            origin_x,
            origin_y,
            width,
            height,
        }))
    })
}

#[test]
fn coverage_must_lie_inside_the_image() {
    // 960x540 at (960, 540) ends exactly on the 1920x1080 edge.
    for ok in [(0, 0, 960, 540), (960, 540, 960, 540), (0, 0, 1920, 1080)] {
        let mut p = plane();
        p.apply(&cov(ok.0, ok.1, ok.2, ok.3), SIZE).unwrap();
        assert_eq!(p.coverage.unwrap().width, ok.2);
    }
    // One step past each edge, plus u32 overflow of origin + width.
    for bad in [
        (0, 0, 1921, 1080),
        (0, 0, 1920, 1081),
        (961, 0, 960, 540),
        (0, 541, 960, 540),
        (u32::MAX, 0, 2, 1),
        (0, u32::MAX, 1, 2),
        (1920, 0, 1, 1),
    ] {
        let mut p = plane();
        assert_eq!(
            p.apply(&cov(bad.0, bad.1, bad.2, bad.3), SIZE),
            Err(ImagePlaneError::CoverageOutsideImage {
                image_width: 1920,
                image_height: 1080
            }),
            "{bad:?}"
        );
        assert_eq!(p, plane());
    }
}

#[test]
fn empty_coverage_is_refused_and_none_restores_the_full_image() {
    let mut p = plane();
    assert_eq!(
        p.apply(&cov(0, 0, 0, 10), SIZE),
        Err(ImagePlaneError::CoverageEmpty)
    );
    assert_eq!(
        p.apply(&cov(0, 0, 10, 0), SIZE),
        Err(ImagePlaneError::CoverageEmpty)
    );
    p.apply(&cov(0, 0, 10, 10), SIZE).unwrap();
    assert_eq!(
        p.apply(&with(|e| e.coverage = Some(None)), SIZE),
        Ok(PlateChange::Changed)
    );
    assert_eq!(p.coverage, None);
}

#[test]
fn a_smaller_new_image_refuses_an_existing_coverage() {
    let mut p = plane();
    p.apply(&cov(960, 540, 960, 540), SIZE).unwrap();
    let before = p.clone();
    let rename = with(|e| e.image_name = Some(PathBuf::from("small.png")));
    assert!(matches!(
        p.apply(&rename, (1280, 720)),
        Err(ImagePlaneError::CoverageOutsideImage { .. })
    ));
    assert_eq!(p, before);
    // Replacing the coverage in the same edit makes the whole edit valid.
    let mut both = cov(0, 0, 1280, 720);
    both.image_name = Some(PathBuf::from("small.png"));
    assert_eq!(p.apply(&both, (1280, 720)), Ok(PlateChange::Changed));
}

#[test]
fn a_refused_edit_changes_nothing_even_when_other_fields_are_valid() {
    let mut p = plane();
    let edit = ImagePlaneEdit {
        depth: Some(500.0),
        fit: Some(PlateFit::Fill),
        frame_offset: Some(-3),
        alpha_gain: Some(f32::NAN),
        ..ImagePlaneEdit::default()
    };
    assert_eq!(
        p.apply(&edit, SIZE),
        Err(ImagePlaneError::NotFinite {
            field: "alpha_gain"
        })
    );
    assert_eq!(p, plane());
}

#[test]
fn same_value_edits_are_unchanged_and_real_edits_are_changed() {
    let mut p = plane();
    let same = ImagePlaneEdit {
        depth: Some(100.0),
        alpha_gain: Some(1.0),
        fit: Some(PlateFit::Best),
        frame_offset: Some(0),
        coverage: Some(None),
        ..named()
    };
    assert_eq!(p.apply(&same, SIZE), Ok(PlateChange::Unchanged));
    assert_eq!(
        p.apply(&ImagePlaneEdit::default(), SIZE),
        Ok(PlateChange::Unchanged)
    );
    assert_eq!(
        p.apply(
            &with(|e| e.display_mode = Some(PlateDisplayMode::Alpha)),
            SIZE
        ),
        Ok(PlateChange::Changed)
    );
    assert_eq!(p.display_mode, PlateDisplayMode::Alpha);
}

#[test]
fn negative_zero_equals_zero_and_is_stored_as_positive_zero() {
    let mut p = plane();
    p.apply(&with(|e| e.alpha_gain = Some(0.0)), SIZE).unwrap();
    assert_eq!(
        p.apply(&with(|e| e.alpha_gain = Some(-0.0)), SIZE),
        Ok(PlateChange::Unchanged)
    );
    let mut q = plane();
    assert_eq!(
        q.apply(&with(|e| e.alpha_gain = Some(-0.0)), SIZE),
        Ok(PlateChange::Changed)
    );
    assert!(q.alpha_gain.is_sign_positive());
}

#[test]
fn errors_name_the_field_in_their_text() {
    let mut p = plane();
    let err = p
        .apply(&with(|e| e.frame_cache = Some(0)), SIZE)
        .unwrap_err();
    assert_eq!(err.to_string(), "frame_cache must be between 1 and 1000");
    let err = p
        .apply(&with(|e| e.depth = Some(f32::NAN)), SIZE)
        .unwrap_err();
    assert_eq!(err.to_string(), "depth must be a finite number");
}

#[test]
fn depth_error_text_is_readable_and_the_smallest_subnormal_is_accepted() {
    let mut p = plane();
    let err = p.apply(&with(|e| e.depth = Some(0.0)), SIZE).unwrap_err();
    assert_eq!(
        err.to_string(),
        "depth must be greater than 0 and at most 3.4028235e38"
    );
    let tiny = f32::from_bits(1);
    assert_eq!(
        p.apply(&with(|e| e.depth = Some(tiny)), SIZE),
        Ok(PlateChange::Changed)
    );
    assert_eq!(p.depth, tiny);
}

#[test]
fn an_image_with_no_pixels_is_refused_by_create_and_apply() {
    for (size, name) in [((0, 1080), "image_width"), ((1920, 0), "image_height")] {
        assert_eq!(
            ImagePlane::create(&named(), size),
            Err(ImagePlaneError::ImageSizeZero { field: name })
        );
        let mut p = plane();
        assert_eq!(
            p.apply(&with(|e| e.depth = Some(5.0)), size),
            Err(ImagePlaneError::ImageSizeZero { field: name })
        );
        assert_eq!(p, plane());
    }
    assert_eq!(
        ImagePlaneError::ImageSizeZero {
            field: "image_width"
        }
        .to_string(),
        "image_width must be at least 1"
    );
}

#[test]
fn resetting_coverage_is_unchanged_without_one_and_changed_with_one() {
    let reset = with(|e| e.coverage = Some(None));
    let mut p = plane();
    assert_eq!(p.apply(&reset, SIZE), Ok(PlateChange::Unchanged));
    p.apply(&cov(0, 0, 10, 10), SIZE).unwrap();
    assert_eq!(p.apply(&reset, SIZE), Ok(PlateChange::Changed));
    assert_eq!(p.coverage, None);
}

#[test]
fn the_coverage_refusal_tells_the_user_to_reset_it_in_the_same_edit() {
    let text = ImagePlaneError::CoverageOutsideImage {
        image_width: 1280,
        image_height: 720,
    }
    .to_string();
    assert!(text.contains("1280x720"), "{text}");
    assert!(text.contains("reset it in the same edit"), "{text}");
}

#[test]
fn debug_output_never_contains_the_image_name() {
    let secret = "/home/zed/SECRET_show/plate.0001.png";
    let edit = ImagePlaneEdit {
        image_name: Some(PathBuf::from(secret)),
        ..ImagePlaneEdit::default()
    };
    let p = ImagePlane::create(&edit, SIZE).unwrap();
    for text in [format!("{p:?}"), format!("{edit:?}"), format!("{p:#?}")] {
        assert!(!text.contains("SECRET") && !text.contains("zed"), "{text}");
        assert!(text.contains(&format!("{}", secret.len())), "{text}");
    }
    let errors = [
        ImagePlane::create(&ImagePlaneEdit::default(), SIZE).unwrap_err(),
        ImagePlane::create(&edit, (0, 0)).unwrap_err(),
    ];
    for e in errors {
        let text = format!("{e:?} {e}");
        assert!(!text.contains("SECRET"), "{text}");
    }
}
