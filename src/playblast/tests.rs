#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;

fn plan(
    dir: &Path,
    name: &str,
    start: f64,
    end: f64,
    w: u32,
    h: u32,
    pct: u32,
) -> Result<PlayblastPlan, PlayblastError> {
    PlayblastPlan::new(dir, name, start, end, w, h, pct)
}

fn ok(dir: &Path, start: f64, end: f64) -> PlayblastPlan {
    plan(dir, "shot", start, end, 64, 48, 100).expect("valid plan")
}

fn name_result(name: &str) -> Result<PlayblastPlan, PlayblastError> {
    let dir = tempfile::tempdir().expect("tempdir");
    plan(dir.path(), name, 1.0, 2.0, 8, 8, 100)
}

#[test]
fn reserved_device_names_are_refused_case_insensitively() {
    for name in [
        "con", "CON", "nul", "NUL.png", "Com1", "com9", "lpt9", "LPT1", "aux.txt", "prn",
    ] {
        assert_eq!(
            name_result(name).unwrap_err(),
            PlayblastError::InvalidName,
            "{name}"
        );
    }
    for name in ["com0", "com10", "console", "lpt", "shot-01_a"] {
        assert!(name_result(name).is_ok(), "{name}");
    }
}

#[test]
fn names_with_separators_traversal_or_bad_length_are_refused() {
    let long = "a".repeat(MAX_NAME_BYTES + 1);
    for name in [
        "", "..", "a/b", "a\\b", "../x", "a.b", "a\0b", "\u{e9}", "a b", "con ", &long,
    ] {
        assert_eq!(
            name_result(name).unwrap_err(),
            PlayblastError::InvalidName,
            "{name:?}"
        );
    }
    assert!(name_result(&"a".repeat(MAX_NAME_BYTES)).is_ok());
}

#[test]
fn directory_must_be_an_existing_real_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("f");
    std::fs::write(&file, b"x").expect("file");
    for bad in [
        PathBuf::new(),
        dir.path().join("missing"),
        file,
        dir.path().join("..").join("x"),
    ] {
        assert_eq!(
            plan(&bad, "s", 1.0, 2.0, 8, 8, 100).unwrap_err(),
            PlayblastError::InvalidDirectory
        );
    }
    #[cfg(unix)]
    {
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(dir.path(), &link).expect("symlink");
        assert_eq!(
            plan(&link, "s", 1.0, 2.0, 8, 8, 100).unwrap_err(),
            PlayblastError::InvalidDirectory
        );
    }
}

#[test]
fn range_end_is_inclusive() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert_eq!(ok(dir.path(), 5.0, 5.0).frame_count(), 1);
    let three = ok(dir.path(), 1.0, 3.0);
    assert_eq!(three.frame_count(), 3);
    assert_eq!(three.frames().collect::<Vec<_>>(), vec![1, 2, 3]);
}

#[test]
fn bad_ranges_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (s, e) in [
        (3.0, 2.0),
        (-1.0, 2.0),
        (f64::NAN, 2.0),
        (1.0, f64::INFINITY),
        (1.5, 2.0),
        (1.0, 2.5),
        (0.0, 1e9),
    ] {
        assert_eq!(
            plan(dir.path(), "s", s, e, 8, 8, 100).unwrap_err(),
            PlayblastError::InvalidRange,
            "{s} {e}"
        );
    }
}

#[test]
fn frame_count_cap_is_checked_at_the_boundary() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(plan(dir.path(), "s", 0.0, 99_999.0, 1, 1, 100).is_ok());
    assert_eq!(
        plan(dir.path(), "s", 0.0, 100_000.0, 1, 1, 100).unwrap_err(),
        PlayblastError::TooManyFrames
    );
}

#[test]
fn size_rounding_and_caps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let size = |w, h, p| plan(dir.path(), "s", 1.0, 1.0, w, h, p).map(|p| p.size());
    assert_eq!(size(1, 1, 1), Ok((1, 1)));
    assert_eq!(size(101, 50, 50), Ok((51, 25)));
    assert_eq!(size(100, 30, 33), Ok((33, 10)));
    assert_eq!(size(0, 8, 100), Err(PlayblastError::InvalidSize));
    assert_eq!(size(8, 8, 0), Err(PlayblastError::InvalidSize));
    assert_eq!(size(8, 8, 101), Err(PlayblastError::InvalidSize));
    assert_eq!(size(8193, 8, 100), Err(PlayblastError::TooLarge));
    assert_eq!(size(4096, 4096, 100), Ok((4096, 4096)));
    assert_eq!(size(4097, 4096, 100), Err(PlayblastError::TooLarge));
    assert_eq!(size(8192, 8192, 50), Ok((4096, 4096)));
}

#[test]
fn total_byte_cap_is_checked_at_the_boundary() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(plan(dir.path(), "s", 1.0, 64.0, 4096, 4096, 100).is_ok());
    assert_eq!(
        plan(dir.path(), "s", 1.0, 65.0, 4096, 4096, 100).unwrap_err(),
        PlayblastError::TooLarge
    );
}

#[test]
fn file_names_use_frame_path_padding_and_grow() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ok(dir.path(), 9998.0, 10_000.0);
    let names: Vec<_> = p
        .frames()
        .map(|f| {
            p.frame_file(f)
                .expect("name")
                .file_name()
                .expect("n")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(names, ["shot.9998.png", "shot.9999.png", "shot.10000.png"]);
    let p = plan(dir.path(), "take2", 1.0, 1.0, 8, 8, 100).expect("plan");
    assert_eq!(
        p.frame_file(7).expect("n").file_name().expect("n"),
        "take2.0007.png"
    );
    assert_eq!(p.frame_file(-1), Err(PlayblastError::InvalidRange));
}

#[test]
fn existing_destination_is_refused_before_any_work() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ok(dir.path(), 1.0, 3.0);
    assert_eq!(p.check_no_collisions(), Ok(()));
    std::fs::write(p.frame_file(2).expect("n"), b"x").expect("file");
    assert_eq!(
        p.check_no_collisions(),
        Err(PlayblastError::DestinationExists { frame: 2 })
    );
}

#[cfg(unix)]
#[test]
fn dangling_symlink_counts_as_existing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let p = ok(dir.path(), 1.0, 2.0);
    std::os::unix::fs::symlink(dir.path().join("nowhere"), p.frame_file(1).expect("n"))
        .expect("symlink");
    assert_eq!(
        p.check_no_collisions(),
        Err(PlayblastError::DestinationExists { frame: 1 })
    );
}
