#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::Path;

use super::job::{install_error, no_hard_links};
use super::writer_tests::names;
use super::*;

const W: u32 = 3;
const H: u32 = 2;

type Hook = Box<dyn FnMut(i64)>;

#[derive(Default)]
struct Fake {
    calls: Vec<i64>,
    fail_at: Option<i64>,
    hook: Option<Hook>,
}

impl FrameRenderer for Fake {
    fn render(&mut self, frame: i64, width: u32, height: u32) -> Result<Vec<u8>, PlayblastError> {
        self.calls.push(frame);
        if let Some(hook) = &mut self.hook {
            hook(frame);
        }
        if self.fail_at == Some(frame) {
            return Err(PlayblastError::Render("fake failure"));
        }
        Ok(vec![frame as u8; (width * height * 4) as usize])
    }
}

fn plan(dir: &Path, start: f64, end: f64) -> PlayblastPlan {
    PlayblastPlan::new(dir, "shot", start, end, W, H, 100).expect("plan")
}

fn run(job: &mut PlayblastJob, renderer: &mut Fake) -> JobProgress {
    for _ in 0..100 {
        match job.step(renderer) {
            JobProgress::Running { .. } => {}
            end => return end,
        }
    }
    panic!("job did not end");
}

fn only_complete_pngs(dir: &Path) -> bool {
    names(dir)
        .iter()
        .all(|name| name.starts_with("shot.") && name.ends_with(".png"))
}

#[test]
fn full_run_renders_each_frame_once_ascending_and_installs_every_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 4.0)).expect("start");
    let mut fake = Fake::default();
    assert_eq!(run(&mut job, &mut fake), JobProgress::Finished { total: 4 });
    assert_eq!(fake.calls, [1, 2, 3, 4]);
    assert_eq!(
        names(dir.path()),
        [
            "shot.0001.png",
            "shot.0002.png",
            "shot.0003.png",
            "shot.0004.png"
        ]
    );
    assert_eq!(job.step(&mut fake), JobProgress::Finished { total: 4 });
    assert_eq!(fake.calls.len(), 4);
}

#[test]
fn cancel_mid_job_keeps_only_complete_frames_and_stops_the_writer() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 50.0)).expect("start");
    let mut fake = Fake::default();
    for _ in 0..3 {
        assert!(matches!(job.step(&mut fake), JobProgress::Running { .. }));
    }
    let JobProgress::Cancelled { done } = job.cancel() else {
        panic!("not cancelled")
    };
    assert!(only_complete_pngs(dir.path()));
    assert_eq!(names(dir.path()).len() as u32, done);
    assert_eq!(job.step(&mut fake), JobProgress::Cancelled { done });
    assert_eq!(fake.calls.len(), 3);
}

#[test]
fn dropping_a_running_job_leaves_no_staged_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 50.0)).expect("start");
    job.step(&mut Fake::default());
    drop(job);
    assert!(only_complete_pngs(dir.path()));
}

#[test]
fn an_occupied_destination_is_refused_before_the_first_render() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("shot.0002.png"), b"mine").unwrap();
    let error = PlayblastJob::start(plan(dir.path(), 1.0, 3.0)).err();
    assert_eq!(error, Some(PlayblastError::DestinationExists { frame: 2 }));
    assert_eq!(names(dir.path()), ["shot.0002.png"]);
}

#[test]
fn a_file_created_mid_job_is_kept_and_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let raced = dir.path().join("shot.0003.png");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 5.0)).expect("start");
    let target = raced.clone();
    let mut fake = Fake {
        hook: Some(Box::new(move |frame| {
            if frame == 1 {
                std::fs::write(&target, b"mine").unwrap()
            }
        })),
        ..Fake::default()
    };
    assert_eq!(
        run(&mut job, &mut fake),
        JobProgress::Failed(PlayblastError::DestinationExists { frame: 3 })
    );
    assert_eq!(std::fs::read(&raced).unwrap(), b"mine");
    assert_eq!(
        names(dir.path()),
        ["shot.0001.png", "shot.0002.png", "shot.0003.png"]
    );
}

#[test]
fn a_renderer_failure_keeps_earlier_frames_and_leaves_no_staged_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 5.0)).expect("start");
    let mut fake = Fake {
        fail_at: Some(3),
        ..Fake::default()
    };
    assert_eq!(
        run(&mut job, &mut fake),
        JobProgress::Failed(PlayblastError::Render("fake failure"))
    );
    assert!(only_complete_pngs(dir.path()));
    assert!(names(dir.path()).len() <= 2);
}

#[cfg(unix)]
#[test]
fn a_writer_io_failure_is_reported_and_leaves_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let mut job = PlayblastJob::start(plan(dir.path(), 1.0, 3.0)).expect("start");
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let end = run(&mut job, &mut Fake::default());
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        end,
        JobProgress::Failed(PlayblastError::Io(io::ErrorKind::PermissionDenied))
    );
    assert!(names(dir.path()).is_empty());
}

#[test]
fn install_errors_map_to_typed_errors() {
    let map = |kind| install_error(&io::Error::from(kind), 7);
    assert_eq!(
        map(io::ErrorKind::AlreadyExists),
        PlayblastError::DestinationExists { frame: 7 }
    );
    assert!(matches!(
        map(io::ErrorKind::Unsupported),
        PlayblastError::Unsupported(_)
    ));
    assert_eq!(
        map(io::ErrorKind::PermissionDenied),
        PlayblastError::Io(io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn hard_link_unsupported_raw_codes_are_matched_per_os() {
    assert!(no_hard_links(1, "linux") && !no_hard_links(2, "linux"));
    assert!(
        no_hard_links(1, "windows") && no_hard_links(50, "windows") && !no_hard_links(5, "windows")
    );
    assert!(!no_hard_links(1, "macos"));
}

#[cfg(unix)]
#[test]
fn start_rechecks_a_directory_swapped_for_a_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("out");
    std::fs::create_dir(&sub).unwrap();
    let planned = plan(&sub, 1.0, 2.0);
    std::fs::remove_dir(&sub).unwrap();
    std::os::unix::fs::symlink(dir.path(), &sub).unwrap();
    assert_eq!(
        PlayblastJob::start(planned).err(),
        Some(PlayblastError::InvalidDirectory)
    );
}
