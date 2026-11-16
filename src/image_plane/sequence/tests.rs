#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::{Path, PathBuf};

use super::frame_path;
use crate::image_plane::decode::PlateError;

fn named(template: &str, frame: i64) -> Result<PathBuf, PlateError> {
    frame_path(Path::new(template), frame)
}

#[test]
fn dotted_underscored_and_bare_digit_runs_keep_their_padding() {
    assert_eq!(
        named("shots/plate.0001.png", 5),
        Ok("shots/plate.0005.png".into())
    );
    assert_eq!(named("plate_0001.png", 12), Ok("plate_0012.png".into()));
    assert_eq!(named("plate0001.png", 1), Ok("plate0001.png".into()));
}

#[test]
fn the_last_digit_run_wins() {
    assert_eq!(named("a1b.0099.png", 100), Ok("a1b.0100.png".into()));
    assert_eq!(
        named("v2_plate.0003.jpg", 4),
        Ok("v2_plate.0004.jpg".into())
    );
}

#[test]
fn an_unpadded_run_grows_with_the_number() {
    assert_eq!(named("plate.9.png", 10), Ok("plate.10.png".into()));
    assert_eq!(
        named("plate.99999.png", 100_000),
        Ok("plate.100000.png".into())
    );
    assert_eq!(
        named("plate.0009.png", 100_000),
        Ok("plate.100000.png".into())
    );
}

#[test]
fn directory_digits_are_never_touched() {
    assert_eq!(
        named("/r2d2/plate.0004.png", 7),
        Ok("/r2d2/plate.0007.png".into())
    );
    assert_eq!(named("/shot01/plate.png", 3), Err(PlateError::Missing));
}

#[test]
fn a_negative_frame_or_a_digitless_name_is_missing() {
    assert_eq!(named("plate.0001.png", -1), Err(PlateError::Missing));
    assert_eq!(named("plate.png", 1), Err(PlateError::Missing));
    assert_eq!(named("plate.0001.png", 0), Ok("plate.0000.png".into()));
}

#[test]
fn a_name_with_no_file_component_is_refused_typed() {
    assert_eq!(
        named("..", 1),
        Err(PlateError::Io(std::io::ErrorKind::InvalidInput))
    );
    assert_eq!(
        named("/", 1),
        Err(PlateError::Io(std::io::ErrorKind::InvalidInput))
    );
}

#[cfg(unix)]
#[test]
fn a_non_utf8_name_is_refused_typed() {
    use std::os::unix::ffi::OsStrExt;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"plate\xff.0001.png"));
    assert_eq!(
        frame_path(path, 2),
        Err(PlateError::Io(std::io::ErrorKind::InvalidInput))
    );
}

#[test]
fn the_frame_number_cannot_introduce_a_separator() {
    let result = named("a/..1.png", 7).unwrap();
    assert_eq!(result, Path::new("a/..7.png"));
    assert_eq!(result.parent(), Some(Path::new("a")));
}

#[test]
fn the_extension_never_supplies_the_frame_number() {
    assert_eq!(named("shot.0001.mp4", 7), Ok("shot.0007.mp4".into()));
    assert_eq!(named("plate.png", 1), Err(PlateError::Missing));
    assert_eq!(named("clip.mp4", 1), Err(PlateError::Missing));
}

#[test]
fn the_last_run_within_the_stem_wins() {
    // PROVISIONAL: with digits after the numbered run, the last run is the frame number.
    assert_eq!(
        named("plate_0001_v2.png", 5),
        Ok("plate_0001_v5.png".into())
    );
}
