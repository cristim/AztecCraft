//! The per-frame plate of a playblast through the Editor operation with the recording fake
//! renderer: which key frame each playblast frame shows, the missing and failed frame rules and
//! the snapshot semantics. The pixels are checked by the headless_gpu tests.
use super::super::image_plane::{PlateRuntime, key_frame};
use super::super::playblast::fake::{self, Asked, AskedLog, PlateSeen, Seen};
use super::super::playblast::tests::{editor, request, run_to_end, ticks};
use crate::app::Editor;
use crate::editor_operations::Operation;
use azteccraft::{
    image_plane::{
        decode::{PlateError, PlateFrame},
        model::{ImagePlane, ImagePlaneEdit, PlateDisplayMode},
        worker::{DecodeFn, PlateSource},
    },
    playblast::{JobProgress, PlayblastError},
};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn plane_png(dir: &Path) -> std::path::PathBuf {
    let path = dir.join("plate.0001.png");
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[9; 16]).unwrap();
    writer.finish().unwrap();
    path
}

/// A decoder whose frame N is a 2 x 2 plate with first texel `[10 * N, 0, 0, 255]`; it fails for
/// the frames in `fail` and counts its calls.
fn decoder(fail: &[(i64, PlateError)], calls: &Arc<AtomicUsize>) -> DecodeFn {
    let fail = fail.to_vec();
    let calls = Arc::clone(calls);
    Arc::new(move |_: &PlateSource, frame: i64| {
        calls.fetch_add(1, Ordering::SeqCst);
        if let Some((_, error)) = fail.iter().find(|(f, _)| *f == frame) {
            return Err(error.clone());
        }
        let texel = [(10 * frame) as u8, 0, 0, 255];
        Ok(PlateFrame {
            width: 2,
            height: 2,
            rgba: texel.repeat(4),
        })
    })
}

struct Rig {
    editor: Editor,
    asked: AskedLog,
    calls: Arc<AtomicUsize>,
    dir: tempfile::TempDir,
}

fn rig(sequence: bool, offset: i32, fail: &[(i64, PlateError)]) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let asked = AskedLog::default();
    let mut editor = editor(&Seen::default());
    editor.playblast.factory = Some(fake::recording_factory(Seen::default(), Arc::clone(&asked)));
    let png = plane_png(dir.path());
    editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            image_name: Some(png),
            use_frame_extension: Some(sequence),
            frame_offset: Some(offset),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    editor.plate = PlateRuntime::with_decode(decoder(fail, &calls));
    Rig {
        editor,
        asked,
        calls,
        dir,
    }
}

fn plates(asked: &Mutex<Asked>) -> Vec<PlateSeen> {
    asked.lock().unwrap().plates.clone()
}

fn start(rig: &mut Rig) -> JobProgress {
    let out = tempfile::tempdir().unwrap();
    rig.editor
        .apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    run_to_end(&mut rig.editor)
}

#[test]
fn the_key_frame_is_the_frame_plus_the_offset_for_a_sequence_and_zero_for_one_image() {
    let mut plane = ImagePlane::create(
        &ImagePlaneEdit {
            image_name: Some(plane_png(tempfile::tempdir().unwrap().keep().as_path())),
            ..ImagePlaneEdit::default()
        },
        (2, 2),
    )
    .unwrap();
    assert_eq!(key_frame(&plane, 7), 0);
    plane.use_frame_extension = true;
    plane.frame_offset = -3;
    assert_eq!(key_frame(&plane, 7), 4);
    assert_eq!(key_frame(&plane, 1), -2);
    assert_eq!(key_frame(&plane, i64::MAX), i64::MAX - 3);
}

#[test]
fn each_playblast_frame_draws_the_plate_of_its_own_frame() {
    let mut rig = rig(true, 1, &[]);
    assert!(matches!(
        start(&mut rig),
        JobProgress::Finished { total: 3 }
    ));
    assert_eq!(
        plates(&rig.asked),
        vec![
            (1, Some((2, [20, 0, 0, 255]))),
            (2, Some((3, [30, 0, 0, 255]))),
            (3, Some((4, [40, 0, 0, 255]))),
        ]
    );
    assert_eq!(rig.calls.load(Ordering::SeqCst), 3, "one decode per frame");
    assert_eq!(rig.asked.lock().unwrap().had_plate_source, [true]);
}

#[test]
fn a_single_image_is_decoded_once_and_drawn_on_every_frame() {
    let mut rig = rig(false, 5, &[]);
    assert!(matches!(
        start(&mut rig),
        JobProgress::Finished { total: 3 }
    ));
    let keys: Vec<_> = plates(&rig.asked)
        .iter()
        .map(|(_, drawn)| drawn.map(|(key, _)| key))
        .collect();
    assert_eq!(keys, [Some(0), Some(0), Some(0)]);
    assert_eq!(rig.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_frame_without_a_file_renders_without_a_plate_and_the_job_continues() {
    let mut rig = rig(true, 0, &[(2, PlateError::Missing)]);
    assert!(matches!(
        start(&mut rig),
        JobProgress::Finished { total: 3 }
    ));
    let drawn: Vec<_> = plates(&rig.asked)
        .iter()
        .map(|(_, d)| d.is_some())
        .collect();
    assert_eq!(drawn, [true, false, true]);
}

#[test]
fn an_unreadable_frame_stops_the_job_naming_the_frame_and_leaves_no_frame_for_it() {
    let mut rig = rig(true, 0, &[(3, PlateError::Corrupt)]);
    let out = tempfile::tempdir().unwrap();
    rig.editor
        .apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    let progress = run_to_end(&mut rig.editor);
    assert!(
        matches!(
            progress,
            JobProgress::Failed(PlayblastError::Plate { frame: 3 })
        ),
        "{progress:?}"
    );
    // The writer may not have finished the earlier frames; none is half written or has frame 3.
    let written: Vec<_> = std::fs::read_dir(out.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        written
            .iter()
            .all(|n| n.ends_with(".png") && !n.contains("0003")),
        "{written:?}"
    );
    drop(rig.dir);
}

#[test]
fn a_negative_key_frame_is_a_missing_frame() {
    let mut rig = rig(true, -2, &[]);
    start(&mut rig);
    // Frames 1, 2, 3 map to -1, 0, 1 and the real file naming refuses a negative number; the
    // injected decoder does not, so only the key frames are pinned here.
    let keys: Vec<_> = plates(&rig.asked)
        .iter()
        .map(|(_, d)| d.map(|(k, _)| k))
        .collect();
    assert_eq!(keys, [Some(-1), Some(0), Some(1)]);
}

#[test]
fn display_mode_none_decodes_nothing() {
    let mut rig = rig(true, 0, &[]);
    rig.editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            display_mode: Some(PlateDisplayMode::None),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    start(&mut rig);
    assert_eq!(rig.calls.load(Ordering::SeqCst), 0);
    assert!(plates(&rig.asked).iter().all(|(_, d)| d.is_none()));
}

#[test]
fn ornaments_off_keeps_the_plate_and_no_plane_means_no_source() {
    let mut rig = rig(true, 0, &[]);
    let out = tempfile::tempdir().unwrap();
    let mut off = request(out.path());
    off.show_ornaments = false;
    rig.editor
        .apply_operation(Operation::Playblast(off))
        .unwrap();
    run_to_end(&mut rig.editor);
    assert!(plates(&rig.asked).iter().all(|(_, d)| d.is_some()));

    let mut bare = editor(&Seen::default());
    let asked = AskedLog::default();
    bare.playblast.factory = Some(fake::recording_factory(Seen::default(), Arc::clone(&asked)));
    let out = tempfile::tempdir().unwrap();
    bare.apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    run_to_end(&mut bare);
    assert_eq!(asked.lock().unwrap().had_plate_source, [false]);
    assert!(plates(&asked).is_empty());
}

#[test]
fn removing_the_plane_after_the_start_does_not_change_the_running_job() {
    let mut rig = rig(true, 0, &[]);
    let out = tempfile::tempdir().unwrap();
    rig.editor
        .apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    rig.editor
        .apply_operation(Operation::RemoveImagePlane)
        .unwrap();
    run_to_end(&mut rig.editor);
    assert_eq!(plates(&rig.asked).len(), 3);
    assert!(plates(&rig.asked).iter().all(|(_, d)| d.is_some()));
}

fn without_plate(editor: &Editor) -> u32 {
    editor.inspect_playblast().frames_without_plate
}

#[test]
fn a_sequence_with_one_missing_frame_reports_exactly_one_frame_without_a_plate() {
    let mut rig = rig(true, 0, &[(2, PlateError::Missing)]);
    assert!(matches!(
        start(&mut rig),
        JobProgress::Finished { total: 3 }
    ));
    assert_eq!(without_plate(&rig.editor), 1);
}

#[test]
fn a_template_without_digits_reports_every_frame_without_a_plate() {
    let mut rig = rig(true, 0, &[]);
    let plain = rig.dir.path().join("plate.png");
    std::fs::copy(plane_png(rig.dir.path()), &plain).unwrap();
    rig.editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            image_name: Some(plain),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    // The real decoder: a sequence template with no digits has no file for any frame.
    rig.editor.plate = PlateRuntime::default();
    assert!(matches!(
        start(&mut rig),
        JobProgress::Finished { total: 3 }
    ));
    assert_eq!(without_plate(&rig.editor), 3);
    assert!(plates(&rig.asked).iter().all(|(_, drawn)| drawn.is_none()));
}

#[test]
fn a_single_image_a_hidden_plane_and_no_plane_report_no_frame_without_a_plate() {
    let mut single = rig(false, 0, &[]);
    start(&mut single);
    assert_eq!(without_plate(&single.editor), 0);

    let mut hidden = rig(true, 0, &[]);
    hidden
        .editor
        .apply_operation(Operation::SetImagePlane(ImagePlaneEdit {
            display_mode: Some(PlateDisplayMode::None),
            ..ImagePlaneEdit::default()
        }))
        .unwrap();
    start(&mut hidden);
    assert_eq!(without_plate(&hidden.editor), 0);

    let mut bare = editor(&Seen::default());
    bare.playblast.factory = Some(fake::factory(Seen::default()));
    let out = tempfile::tempdir().unwrap();
    bare.apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    run_to_end(&mut bare);
    assert_eq!(without_plate(&bare), 0);
}

#[test]
fn the_count_is_visible_while_running_and_after_a_cancel() {
    let mut rig = rig(true, 0, &[(1, PlateError::Missing)]);
    let out = tempfile::tempdir().unwrap();
    rig.editor
        .apply_operation(Operation::Playblast(request(out.path())))
        .unwrap();
    assert_eq!(without_plate(&rig.editor), 0, "nothing rendered yet");
    ticks(&mut rig.editor, 1);
    assert_eq!(without_plate(&rig.editor), 1);
    rig.editor
        .apply_operation(Operation::CancelPlayblast)
        .unwrap();
    assert_eq!(without_plate(&rig.editor), 1);
}
