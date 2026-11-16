#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::*;
use crate::image_plane::decode::MAX_PLATE_FILE_BYTES;

type Calls = Arc<Mutex<Vec<i64>>>;

fn source(name: &str) -> PlateSource {
    PlateSource {
        template: PathBuf::from(name),
        sequence: true,
    }
}

/// Pixel value: `[frame, first byte of the source name, 0, 255]`, so a test can tell which
/// source and frame a result came from.
fn pixels(source: &PlateSource, frame: i64) -> PlateFrame {
    let tag = source
        .template
        .to_string_lossy()
        .bytes()
        .next()
        .unwrap_or(0);
    PlateFrame {
        width: 1,
        height: 1,
        rgba: vec![frame as u8, tag, 0, 255],
    }
}

/// A decode function that records the frames it is asked for and, when given a gate, blocks
/// its first call until the gate is released.
fn recording(calls: &Calls, gate: Option<Receiver<()>>) -> DecodeFn {
    let calls = Arc::clone(calls);
    let gate = Mutex::new(gate);
    Arc::new(move |source, frame| {
        let first = {
            let mut calls = calls.lock().unwrap();
            calls.push(frame);
            calls.len() == 1
        };
        if first && let Some(gate) = gate.lock().unwrap().take() {
            let _ = gate.recv_timeout(Duration::from_secs(20));
        }
        Ok(pixels(source, frame))
    })
}

fn counting_wake(wakes: &Arc<AtomicUsize>) -> WakeFn {
    let wakes = Arc::clone(wakes);
    Arc::new(move || {
        wakes.fetch_add(1, Ordering::SeqCst);
    })
}

fn until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "condition not reached in time");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn stream(decode: DecodeFn) -> (PlateStream, Arc<AtomicUsize>) {
    let wakes = Arc::new(AtomicUsize::new(0));
    (PlateStream::new(decode, counting_wake(&wakes), 24), wakes)
}

fn ready(stream: &mut PlateStream, wanted: &[i64], frame: i64) -> Arc<PlateFrame> {
    let mut found = None;
    until(|| {
        stream.tick(wanted);
        if let FrameState::Ready(pixels) = stream.state(frame) {
            found = Some(pixels);
        }
        found.is_some()
    });
    found.unwrap()
}

#[test]
fn a_wanted_frame_reaches_ready_with_its_own_pixels() {
    let calls = Calls::default();
    let (mut stream, wakes) = stream(recording(&calls, None));
    stream.set_source(source("a"));
    let frame = ready(&mut stream, &[5], 5);
    assert_eq!(frame.rgba, [5, b'a', 0, 255]);
    assert!(wakes.load(Ordering::SeqCst) >= 1);
}

#[test]
fn a_scrub_decodes_only_the_newest_window() {
    let calls = Calls::default();
    let (release, gate) = channel();
    let (mut stream, _) = stream(recording(&calls, Some(gate)));
    stream.set_source(source("a"));
    stream.tick(&[0, 1, 2]);
    until(|| calls.lock().unwrap().len() == 1);
    for scrub in 1..=100 {
        stream.tick(&[scrub * 10, scrub * 10 + 1, scrub * 10 + 2]);
    }
    stream.tick(&[1000, 1001]);
    release.send(()).unwrap();
    ready(&mut stream, &[1000, 1001], 1001);
    // Frame 0 was already decoding; frames 1 and 2 and every intermediate window never ran.
    assert_eq!(*calls.lock().unwrap(), [0, 1000, 1001]);
}

#[test]
fn a_decode_held_across_a_source_change_never_shows_the_old_pixels() {
    let calls = Calls::default();
    let (release, gate) = channel();
    let (mut stream, _) = stream(recording(&calls, Some(gate)));
    stream.set_source(source("a"));
    stream.tick(&[0]);
    until(|| calls.lock().unwrap().len() == 1);
    stream.set_source(source("b"));
    assert!(matches!(stream.state(0), FrameState::Loading));
    release.send(()).unwrap();
    let mut shown = None;
    until(|| {
        stream.tick(&[0]);
        if let FrameState::Ready(pixels) = stream.state(0) {
            shown = Some(pixels);
        }
        shown.is_some()
    });
    assert_eq!(shown.unwrap().rgba[1], b'b');
    // The stale result from source "a" was dropped on arrival, not stored.
    until(|| {
        stream.tick(&[0]);
        calls.lock().unwrap().len() == 2
    });
    assert_eq!(stream.cache().len(), 1);
}

#[test]
fn a_dropped_slot_write_is_repaired_by_the_next_tick() {
    let calls = Calls::default();
    let (release, gate) = channel();
    let (mut stream, _) = stream(recording(&calls, Some(gate)));
    stream.set_source(source("a"));
    stream.tick(&[0]);
    until(|| calls.lock().unwrap().len() == 1);
    stream.tick(&[7]);
    // The write for frame 7 is lost before the worker sees it.
    lock(&stream.shared).unwrap().window.clear();
    release.send(()).unwrap();
    let frame = ready(&mut stream, &[7], 7);
    assert_eq!(frame.rgba[0], 7);
}

#[test]
fn nothing_is_written_woken_or_decoded_once_the_window_is_settled() {
    let calls = Calls::default();
    let (mut stream, wakes) = stream(recording(&calls, None));
    stream.set_source(source("a"));
    ready(&mut stream, &[3], 3);
    assert!(!stream.tick(&[3]));
    let version = lock(&stream.shared).unwrap().version;
    let (wake_count, call_count) = (wakes.load(Ordering::SeqCst), calls.lock().unwrap().len());
    for _ in 0..1000 {
        assert!(!stream.tick(&[3]));
    }
    assert_eq!(lock(&stream.shared).unwrap().version, version);
    assert_eq!(wakes.load(Ordering::SeqCst), wake_count);
    assert_eq!(calls.lock().unwrap().len(), call_count);
}

#[test]
fn tick_never_blocks_while_a_decode_is_stuck() {
    let calls = Calls::default();
    let (release, gate) = channel();
    let (mut stream, _) = stream(recording(&calls, Some(gate)));
    stream.set_source(source("a"));
    let started = Instant::now();
    for frame in 0..200 {
        assert!(stream.tick(&[frame, frame + 1]));
    }
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    release.send(()).unwrap();
}

#[test]
fn a_panicking_decode_fails_that_frame_and_the_worker_goes_on() {
    let decode: DecodeFn = Arc::new(|source, frame| {
        if frame == 1 {
            panic!("decoder bug");
        }
        Ok(pixels(source, frame))
    });
    let (mut stream, _) = stream(decode);
    stream.set_source(source("a"));
    ready(&mut stream, &[1, 2], 2);
    assert!(matches!(
        stream.state(1),
        FrameState::Failed(PlateError::Internal)
    ));
    assert!(stream.worker_alive());
    assert_eq!(stream.spawned_workers(), 1);
}

#[test]
fn missing_and_failed_frames_are_distinct_states() {
    let decode: DecodeFn = Arc::new(|source, frame| match frame {
        4 => Err(PlateError::Missing),
        5 => Err(PlateError::Corrupt),
        _ => Ok(pixels(source, frame)),
    });
    let (mut stream, _) = stream(decode);
    stream.set_source(source("a"));
    ready(&mut stream, &[4, 5, 6], 6);
    assert!(matches!(stream.state(4), FrameState::Missing));
    assert!(matches!(
        stream.state(5),
        FrameState::Failed(PlateError::Corrupt)
    ));
}

#[test]
fn a_hundred_sources_in_a_row_spawn_one_thread() {
    let calls = Calls::default();
    let (mut stream, _) = stream(recording(&calls, None));
    for n in 0..100 {
        stream.set_source(source(&format!("s{n}")));
    }
    assert_eq!(stream.spawned_workers(), 1);
    assert_eq!(stream.generation(), 100);
    let frame = ready(&mut stream, &[1], 1);
    assert_eq!(frame.rgba[1], b's');
    assert_eq!(stream.spawned_workers(), 1);
}

#[test]
fn a_killed_worker_fails_pending_frames_and_a_new_source_replaces_it_once() {
    let calls = Calls::default();
    let kill = Arc::new(AtomicBool::new(true));
    let wake: WakeFn = {
        let kill = Arc::clone(&kill);
        Arc::new(move || {
            if kill.load(Ordering::SeqCst) {
                panic!("wake callback bug escapes the decode guard");
            }
        })
    };
    let mut stream = PlateStream::new(recording(&calls, None), wake, 24);
    stream.set_source(source("a"));
    stream.tick(&[1]);
    until(|| !stream.worker_alive());
    stream.tick(&[1, 2]);
    assert!(matches!(
        stream.state(2),
        FrameState::Failed(PlateError::WorkerGone)
    ));
    kill.store(false, Ordering::SeqCst);
    stream.set_source(source("b"));
    stream.set_source(source("c"));
    assert_eq!(stream.spawned_workers(), 2);
    assert_eq!(ready(&mut stream, &[2], 2).rgba[1], b'c');
    assert_eq!(stream.spawned_workers(), 2);
}

#[test]
fn dropping_the_stream_never_waits_for_a_blocked_worker() {
    let calls = Calls::default();
    let (release, gate) = channel();
    let (mut stream, _) = stream(recording(&calls, Some(gate)));
    stream.set_source(source("a"));
    stream.tick(&[0]);
    until(|| calls.lock().unwrap().len() == 1);
    let started = Instant::now();
    drop(stream);
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    release.send(()).unwrap();
}

#[test]
fn removing_the_source_forgets_everything_and_stops_requesting() {
    let calls = Calls::default();
    let (mut stream, _) = stream(recording(&calls, None));
    stream.set_source(source("a"));
    ready(&mut stream, &[1], 1);
    stream.remove_source();
    assert_eq!(stream.cache().len(), 0);
    assert!(!stream.tick(&[1, 2, 3]));
}

fn encode_png(path: &std::path::Path, value: u8) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 1, 1);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[value, 0, 0, 255]).unwrap();
}

#[test]
fn a_real_sequence_decodes_frame_by_frame_and_a_gap_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    encode_png(&dir.path().join("plate.0001.png"), 10);
    encode_png(&dir.path().join("plate.0002.png"), 20);
    let template = PlateSource {
        template: dir.path().join("plate.0001.png"),
        sequence: true,
    };
    assert_eq!(
        decode_from_disk(&template, 2).unwrap().rgba,
        [20, 0, 0, 255]
    );
    assert_eq!(decode_from_disk(&template, 3), Err(PlateError::Missing));
    assert_eq!(decode_from_disk(&template, -1), Err(PlateError::Missing));
    let single = PlateSource {
        sequence: false,
        ..template
    };
    assert_eq!(decode_from_disk(&single, 99).unwrap().rgba, [10, 0, 0, 255]);
}

#[test]
fn an_oversize_file_is_refused_without_being_read() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.png");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_PLATE_FILE_BYTES as u64 + 1).unwrap();
    assert_eq!(read_plate_file(&path), Err(PlateError::TooLarge));
}

#[cfg(unix)]
#[test]
fn a_fifo_and_a_symlink_loop_are_refused_typed() {
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("plate.0001.png");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        read_plate_file(&fifo),
        Err(PlateError::Io(std::io::ErrorKind::InvalidInput))
    );
    let (a, b) = (dir.path().join("a.png"), dir.path().join("b.png"));
    std::os::unix::fs::symlink(&b, &a).unwrap();
    std::os::unix::fs::symlink(&a, &b).unwrap();
    assert!(matches!(read_plate_file(&a), Err(PlateError::Io(_))));
    let directory = dir.path().join("d.png");
    std::fs::create_dir(&directory).unwrap();
    assert!(matches!(
        read_plate_file(&directory),
        Err(PlateError::Io(_))
    ));
}

fn hundred_byte_frames() -> DecodeFn {
    Arc::new(|_, frame| {
        Ok(PlateFrame {
            width: 1,
            height: 25,
            rgba: vec![frame as u8; 100],
        })
    })
}

#[test]
fn a_repeated_oversized_window_never_takes_the_cache_over_its_budget() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut stream =
        PlateStream::with_budget(hundred_byte_frames(), counting_wake(&wakes), 24, 400);
    stream.set_source(source("a"));
    let window: Vec<i64> = (0..20).collect();
    until(|| {
        stream.tick(&window);
        (0..4).all(|frame| matches!(stream.state(frame), FrameState::Ready(_)))
    });
    for _ in 0..300 {
        stream.tick(&window);
        assert!(
            stream.cache().total_bytes() <= 400,
            "{}",
            stream.cache().total_bytes()
        );
        assert!(stream.cache().len() <= 4, "{}", stream.cache().len());
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        (stream.cache().len(), stream.cache().total_bytes()),
        (4, 400)
    );
}

#[test]
fn the_frame_on_screen_stays_cached_while_the_window_moves() {
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut stream =
        PlateStream::with_budget(hundred_byte_frames(), counting_wake(&wakes), 24, 200);
    stream.set_source(source("a"));
    ready(&mut stream, &[0], 0);
    for next in 1..=6 {
        ready(&mut stream, &[0, next], next);
        // `contains` does not refresh recency, so only the pin keeps frame 0.
        let on_screen = FrameKey {
            generation: stream.generation(),
            frame: 0,
        };
        assert!(
            stream.cache().contains(on_screen),
            "frame 0 evicted at {next}"
        );
        assert!(stream.cache().total_bytes() <= 200);
    }
}

/// 100-byte frames that count their calls.
fn counting_hundred_byte_frames(calls: &Calls) -> DecodeFn {
    let calls = Arc::clone(calls);
    Arc::new(move |_, frame| {
        calls.lock().unwrap().push(frame);
        Ok(PlateFrame {
            width: 1,
            height: 25,
            rgba: vec![frame as u8; 100],
        })
    })
}

#[test]
fn an_oversized_window_is_clipped_to_the_capacity_and_then_goes_quiet() {
    let calls = Calls::default();
    let wakes = Arc::new(AtomicUsize::new(0));
    let decode = counting_hundred_byte_frames(&calls);
    let mut stream = PlateStream::with_budget(decode, counting_wake(&wakes), 24, 400);
    stream.set_source(source("a"));
    let window: Vec<i64> = (0..20).collect();
    until(|| {
        stream.tick(&window);
        (0..4).all(|frame| matches!(stream.state(frame), FrameState::Ready(_)))
    });
    for _ in 0..200 {
        stream.tick(&window);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!stream.tick(&window), "nothing is outstanding once settled");
    // Only the frame on screen is requested until its result fixes the capacity at 4, so the
    // count does not depend on how the scheduler interleaves the threads.
    let decoded = calls.lock().unwrap().len();
    assert_eq!(decoded, 4, "decoded {decoded} of 20");
}

#[test]
fn a_new_source_is_sized_by_its_own_first_frame_not_the_old_one() {
    let calls: Arc<Mutex<Vec<(PathBuf, i64)>>> = Arc::default();
    let recorded = Arc::clone(&calls);
    let decode: DecodeFn = Arc::new(move |source, frame| {
        recorded
            .lock()
            .unwrap()
            .push((source.template.clone(), frame));
        let height = if source.template.as_os_str() == "a" {
            2
        } else {
            25
        };
        Ok(PlateFrame {
            width: 1,
            height,
            rgba: vec![0; height as usize * 4],
        })
    });
    let wakes = Arc::new(AtomicUsize::new(0));
    let mut stream = PlateStream::with_budget(decode, counting_wake(&wakes), 24, 400);
    let window: Vec<i64> = (0..20).collect();
    stream.set_source(source("a"));
    until(|| !stream.tick(&window));
    stream.set_source(source("b"));
    until(|| {
        stream.tick(&window);
        (0..4).all(|frame| matches!(stream.state(frame), FrameState::Ready(_)))
    });
    for _ in 0..200 {
        stream.tick(&window);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!stream.tick(&window), "nothing is outstanding once settled");
    let decoded_b = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(template, _)| template.as_os_str() == "b")
        .count();
    assert_eq!(decoded_b, 4, "source b decoded {decoded_b} of 20");
}

#[test]
fn a_frame_of_another_size_fails_as_a_dimension_mismatch() {
    let decode: DecodeFn = Arc::new(|_, frame| {
        let height = if frame == 1 { 50 } else { 25 };
        Ok(PlateFrame {
            width: 1,
            height,
            rgba: vec![0; height as usize * 4],
        })
    });
    let (mut stream, _) = stream(decode);
    stream.set_source(source("a"));
    ready(&mut stream, &[0, 1, 2], 2);
    assert!(matches!(
        stream.state(1),
        FrameState::Failed(PlateError::DimensionMismatch)
    ));
    assert_eq!(stream.cache().total_bytes(), 200);
    // A new source may have another size.
    stream.set_source(source("b"));
    assert_eq!(ready(&mut stream, &[1], 1).height, 50);
}
