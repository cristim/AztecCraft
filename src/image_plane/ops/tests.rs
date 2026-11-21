#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::{Path, PathBuf};

use super::{
    ImagePlaneOpError, ImagePlaneState, MAX_IMAGE_NAME_BYTES, check_image_name, remove_image_plane,
    set_image_plane,
};
use crate::image_plane::decode::{MAX_PLATE_FILE_BYTES, PlateError, PlateFormat};
use crate::image_plane::model::{Coverage, ImagePlaneEdit, ImagePlaneError, PlateChange, PlateFit};

fn write_png(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let path = dir.join(name);
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer
        .write_image_data(&vec![0u8; (width * height * 4) as usize])
        .unwrap();
    writer.finish().unwrap();
    path
}

fn named(path: &Path) -> ImagePlaneEdit {
    ImagePlaneEdit {
        image_name: Some(path.to_path_buf()),
        ..ImagePlaneEdit::default()
    }
}

fn with_plane(dir: &Path) -> (Option<ImagePlaneState>, PathBuf) {
    let path = write_png(dir, "plate.0001.png", 4, 2);
    let mut state = None;
    assert_eq!(
        set_image_plane(&mut state, &named(&path)),
        Ok(PlateChange::Changed)
    );
    (state, path)
}

#[test]
fn create_probes_the_first_frame_and_stores_its_size_and_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let (state, path) = with_plane(dir.path());
    let state = state.unwrap();
    assert_eq!(state.image_size, (4, 2));
    assert_eq!(state.plane.image_name, path);
    assert_eq!(state.plane.depth, 100.0);
}

#[test]
fn an_empty_edit_is_refused_by_the_operation_but_unchanged_by_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = with_plane(dir.path());
    let before = state.clone();
    assert_eq!(
        set_image_plane(&mut state, &ImagePlaneEdit::default()),
        Err(ImagePlaneOpError::EmptyEdit)
    );
    assert_eq!(state, before);
    let mut none = None;
    assert_eq!(
        set_image_plane(&mut none, &ImagePlaneEdit::default()),
        Err(ImagePlaneOpError::EmptyEdit)
    );
    assert!(none.is_none());
    // The model layer: an empty edit on a real plane is Unchanged.
    let mut plane = state.unwrap().plane;
    assert_eq!(
        plane.apply(&ImagePlaneEdit::default(), (4, 2)),
        Ok(PlateChange::Unchanged)
    );
}

#[test]
fn creating_without_a_name_is_refused_before_any_probe() {
    let mut state = None;
    let edit = ImagePlaneEdit {
        depth: Some(50.0),
        ..ImagePlaneEdit::default()
    };
    assert_eq!(
        set_image_plane(&mut state, &edit),
        Err(ImagePlaneOpError::Model(ImagePlaneError::ImageNameMissing))
    );
    assert!(state.is_none());
}

#[test]
fn a_depth_only_edit_changes_the_plane_without_reading_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, path) = with_plane(dir.path());
    std::fs::remove_file(&path).unwrap();
    let edit = ImagePlaneEdit {
        depth: Some(250.0),
        ..ImagePlaneEdit::default()
    };
    assert_eq!(set_image_plane(&mut state, &edit), Ok(PlateChange::Changed));
    assert_eq!(state.as_ref().unwrap().plane.depth, 250.0);
    assert_eq!(
        set_image_plane(&mut state, &edit),
        Ok(PlateChange::Unchanged)
    );
}

#[test]
fn setting_the_same_name_again_is_unchanged_and_does_not_reload_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, path) = with_plane(dir.path());
    // The file is replaced by a 8x8 image; the same name must not pick it up.
    write_png(dir.path(), "plate.0001.png", 8, 8);
    assert_eq!(
        set_image_plane(&mut state, &named(&path)),
        Ok(PlateChange::Unchanged)
    );
    assert_eq!(state.as_ref().unwrap().image_size, (4, 2));
    // Even with the file gone, the same name plus another field is a plain edit: no probe.
    std::fs::remove_file(&path).unwrap();
    let edit = ImagePlaneEdit {
        depth: Some(7.0),
        ..named(&path)
    };
    assert_eq!(set_image_plane(&mut state, &edit), Ok(PlateChange::Changed));
    assert_eq!(state.unwrap().plane.depth, 7.0);
}

#[test]
fn a_new_name_reloads_the_size() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = with_plane(dir.path());
    let other = write_png(dir.path(), "other.png", 8, 6);
    assert_eq!(
        set_image_plane(&mut state, &named(&other)),
        Ok(PlateChange::Changed)
    );
    assert_eq!(state.unwrap().image_size, (8, 6));
}

#[test]
fn a_bad_file_leaves_state_and_plane_unchanged_and_the_error_has_no_path() {
    let dir = tempfile::tempdir().unwrap();
    let secret = dir.path().join("SECRETSHOW");
    std::fs::create_dir(&secret).unwrap();
    let (mut state, _) = with_plane(dir.path());
    let before = state.clone();
    let missing = secret.join("gone.png");
    let not_image = secret.join("text.png");
    std::fs::write(&not_image, b"this is not an image at all").unwrap();
    let directory = secret.join("dir.png");
    std::fs::create_dir(&directory).unwrap();
    let exr = secret.join("plate.png");
    std::fs::write(&exr, [0x76, 0x2f, 0x31, 0x01, 0, 0, 0, 0]).unwrap();
    for bad in [&missing, &not_image, &directory, &exr] {
        let error = set_image_plane(&mut state, &named(bad)).unwrap_err();
        assert!(matches!(error, ImagePlaneOpError::Plate(_)), "{error:?}");
        let text = format!("{error} {error:?}");
        assert!(!text.contains("SECRETSHOW"), "{text}");
        assert_eq!(state, before);
        let mut none = None;
        assert!(set_image_plane(&mut none, &named(bad)).is_err());
        assert!(none.is_none());
    }
    assert_eq!(
        set_image_plane(&mut state, &named(&exr)).unwrap_err(),
        ImagePlaneOpError::Plate(PlateError::UnsupportedFormat {
            format: PlateFormat::Exr
        })
    );
}

#[test]
fn the_probe_reads_only_the_prefix_up_to_the_size_cap_and_refuses_one_byte_over() {
    let dir = tempfile::tempdir().unwrap();
    let small = write_png(dir.path(), "small.png", 2, 2);
    let sparse = |name: &str, len: u64| {
        let path = dir.path().join(name);
        std::fs::copy(&small, &path).unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(len).unwrap();
        path
    };
    // Exactly at the cap: a sparse file whose whole-file read would still be allowed. Success
    // with 128 MiB of file shows only the prefix was read.
    let at_cap = sparse("at_cap.png", MAX_PLATE_FILE_BYTES as u64);
    let mut state = None;
    assert_eq!(
        set_image_plane(&mut state, &named(&at_cap)),
        Ok(PlateChange::Changed)
    );
    assert_eq!(state.as_ref().unwrap().image_size, (2, 2));
    // One byte over: refused at set time, because every later decode would fail TooLarge.
    let over = sparse("over.png", MAX_PLATE_FILE_BYTES as u64 + 1);
    let before = state.clone();
    assert_eq!(
        set_image_plane(&mut state, &named(&over)),
        Err(ImagePlaneOpError::Plate(PlateError::TooLarge))
    );
    assert_eq!(state, before);
    let mut none = None;
    assert!(set_image_plane(&mut none, &named(&over)).is_err());
    assert!(none.is_none());
}

#[test]
fn a_smaller_new_image_with_a_stored_coverage_is_refused_with_the_reset_hint() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = with_plane(dir.path());
    let covered = ImagePlaneEdit {
        coverage: Some(Some(Coverage {
            origin_x: 2,
            origin_y: 1,
            width: 2,
            height: 1,
        })),
        ..ImagePlaneEdit::default()
    };
    set_image_plane(&mut state, &covered).unwrap();
    let before = state.clone();
    let small = write_png(dir.path(), "small.png", 2, 2);
    let error = set_image_plane(&mut state, &named(&small)).unwrap_err();
    assert!(
        matches!(
            error,
            ImagePlaneOpError::Model(ImagePlaneError::CoverageOutsideImage { .. })
        ),
        "{error:?}"
    );
    assert!(error.to_string().contains("reset it in the same edit"));
    assert_eq!(state, before);
    // The same edit with the coverage reset succeeds.
    let both = ImagePlaneEdit {
        coverage: Some(None),
        ..named(&small)
    };
    assert_eq!(set_image_plane(&mut state, &both), Ok(PlateChange::Changed));
    assert_eq!(state.unwrap().plane.coverage, None);
}

#[test]
fn a_model_refusal_on_an_existing_plane_changes_nothing_and_keeps_the_old_size() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = with_plane(dir.path());
    let before = state.clone();
    let other = write_png(dir.path(), "other.png", 8, 6);
    let edit = ImagePlaneEdit {
        fit: Some(PlateFit::Fill),
        depth: Some(-1.0),
        ..named(&other)
    };
    assert!(matches!(
        set_image_plane(&mut state, &edit),
        Err(ImagePlaneOpError::Model(ImagePlaneError::DepthOutOfRange))
    ));
    assert_eq!(state, before);
}

#[test]
fn path_rules_hold_before_any_open() {
    let ok = |p: &str| check_image_name(Path::new(p));
    assert_eq!(ok("a.png"), Ok(()));
    assert_eq!(ok("a.JPG"), Ok(()));
    assert_eq!(ok("a.JpEg"), Ok(()));
    assert_eq!(ok(""), Err(ImagePlaneOpError::NameLength));
    assert_eq!(ok("a.txt"), Err(ImagePlaneOpError::NameExtension));
    assert_eq!(ok("png"), Err(ImagePlaneOpError::NameExtension));
    assert_eq!(ok("a\0.png"), Err(ImagePlaneOpError::NameHasNul));
    assert_eq!(ok("a.exr"), Err(ImagePlaneOpError::NameExtension));
    let tail = ".png";
    let at_limit = format!("{}{tail}", "a".repeat(MAX_IMAGE_NAME_BYTES - tail.len()));
    assert_eq!(at_limit.len(), MAX_IMAGE_NAME_BYTES);
    assert_eq!(ok(&at_limit), Ok(()));
    let over = format!("a{at_limit}");
    assert_eq!(ok(&over), Err(ImagePlaneOpError::NameLength));
    // A missing .txt is refused for its extension, not as not-found: nothing was opened.
    let mut state = None;
    assert_eq!(
        set_image_plane(&mut state, &named(Path::new("/nonexistent/x.txt"))),
        Err(ImagePlaneOpError::NameExtension)
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_is_refused() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let name = Path::new(OsStr::from_bytes(b"bad\xff.png"));
    assert_eq!(check_image_name(name), Err(ImagePlaneOpError::NameNotUtf8));
}

#[test]
fn a_refused_name_in_a_multi_field_edit_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _) = with_plane(dir.path());
    let before = state.clone();
    let edit = ImagePlaneEdit {
        depth: Some(5.0),
        ..named(Path::new("x.gif"))
    };
    assert_eq!(
        set_image_plane(&mut state, &edit),
        Err(ImagePlaneOpError::NameExtension)
    );
    assert_eq!(state, before);
}

#[test]
fn remove_reports_changed_only_when_a_plane_existed() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, path) = with_plane(dir.path());
    assert_eq!(remove_image_plane(&mut state), PlateChange::Changed);
    assert!(state.is_none());
    assert_eq!(remove_image_plane(&mut state), PlateChange::Unchanged);
    // Re-creating after a remove reloads the file.
    write_png(dir.path(), "plate.0001.png", 8, 8);
    set_image_plane(&mut state, &named(&path)).unwrap();
    assert_eq!(state.unwrap().image_size, (8, 8));
}

#[test]
fn an_empty_edit_reports_empty_through_is_empty() {
    assert!(ImagePlaneEdit::default().is_empty());
    assert!(
        !ImagePlaneEdit {
            coverage: Some(None),
            ..ImagePlaneEdit::default()
        }
        .is_empty()
    );
}

#[test]
fn error_text_never_contains_a_path_for_any_variant() {
    for error in [
        ImagePlaneOpError::EmptyEdit,
        ImagePlaneOpError::NameNotUtf8,
        ImagePlaneOpError::NameHasNul,
        ImagePlaneOpError::NameLength,
        ImagePlaneOpError::NameExtension,
    ] {
        assert!(!error.to_string().contains('/'), "{error}");
    }
}
