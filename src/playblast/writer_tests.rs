#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::io::Cursor;
use std::path::Path;

use super::*;

const W: u32 = 3;
const H: u32 = 2;

pub(super) fn names(dir: &Path) -> Vec<String> {
    let mut all: Vec<_> = std::fs::read_dir(dir)
        .expect("read_dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    all.sort();
    all
}

#[test]
fn install_round_trips_the_png_and_leaves_no_staged_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rgba: Vec<u8> = (0..W * H * 4).map(|n| n as u8).collect();
    let target = dir.path().join("shot.0001.png");
    install_frame(&target, &encode_png(&rgba, W, H).unwrap(), &|| false).expect("install");
    assert_eq!(names(dir.path()), ["shot.0001.png"]);
    let mut reader = png::Decoder::new(Cursor::new(std::fs::read(&target).unwrap()))
        .read_info()
        .unwrap();
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    reader.next_frame(&mut decoded).unwrap();
    assert_eq!(decoded, rgba);
    assert_eq!(
        encode_png(&rgba[1..], W, H),
        Err(PlayblastError::Render("frame size does not match the plan"))
    );
}

#[test]
fn install_refuses_an_existing_entry_and_cleans_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let target = dir.path().join("shot.0001.png");
    std::fs::write(&target, b"mine").unwrap();
    let error = install_frame(&target, b"new", &|| false).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(std::fs::read(&target).unwrap(), b"mine");
    assert_eq!(names(dir.path()), ["shot.0001.png"]);
}

// Real symlinks need unix; the Windows path is the same hard_link call and is not run here.
#[cfg(unix)]
#[test]
fn install_never_writes_through_a_dangling_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let outside = tempfile::tempdir().expect("outside");
    let aim = outside.path().join("escaped.png");
    let target = dir.path().join("shot.0001.png");
    std::os::unix::fs::symlink(&aim, &target).unwrap();
    let error = install_frame(&target, b"png", &|| false).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert!(!aim.exists() && names(outside.path()).is_empty());
    assert_eq!(names(dir.path()), ["shot.0001.png"]);
}

#[test]
fn cancelled_install_removes_the_staged_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let error = install_frame(&dir.path().join("shot.0001.png"), b"png", &|| true).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert!(names(dir.path()).is_empty());
}
