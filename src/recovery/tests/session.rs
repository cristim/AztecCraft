use super::*;

#[test]
fn live_session_is_not_a_crash_and_clean_end_removes_the_marker() {
    let (_tmp, store) = store(10);
    let kept = store.write("a", b"work").unwrap().entry;
    let guard = store.begin_session().unwrap();
    assert!(!guard.replaced_stale_marker);
    assert!(store.crashed_sessions().unwrap().is_empty());
    assert!(matches!(
        store.begin_session(),
        Err(RecoveryError::Contended)
    ));
    assert!(matches!(
        store.clear_crashed_session(4242),
        Err(RecoveryError::Contended)
    ));
    guard.end().unwrap();
    assert!(store.crashed_sessions().unwrap().is_empty());
    assert_eq!(names(&store), vec![kept.file_name]);
}

#[test]
fn unheld_marker_is_a_crash_until_cleared_and_recovery_files_survive() {
    let (_tmp, store) = store(10);
    let kept = store.write("a", b"work").unwrap().entry;
    // A dead session leaves its marker behind with no lock holder.
    fs::write(store.dir().join("session-9001.marker"), b"").unwrap();
    assert_eq!(store.crashed_sessions().unwrap(), vec![9001]);
    assert_eq!(
        store.crashed_sessions().unwrap(),
        vec![9001],
        "detection is read-only"
    );
    store.clear_crashed_session(9001).unwrap();
    store.clear_crashed_session(9001).unwrap();
    assert!(store.crashed_sessions().unwrap().is_empty());
    assert_eq!(store.read(&kept).unwrap(), b"work");
}

#[test]
fn stale_marker_for_our_pid_is_taken_over_and_reported() {
    let (_tmp, store) = store(10);
    fs::create_dir_all(store.dir()).unwrap();
    fs::write(store.dir().join("session-4242.marker"), b"").unwrap();
    assert_eq!(store.crashed_sessions().unwrap(), vec![4242]);
    let guard = store.begin_session().unwrap();
    assert!(guard.replaced_stale_marker);
    // The earlier crash stays visible until acknowledged; the live marker is never probed.
    assert_eq!(store.crashed_sessions().unwrap(), vec![4242]);
    store.clear_crashed_session(4242).unwrap();
    assert!(store.crashed_sessions().unwrap().is_empty());
    assert!(matches!(
        store.clear_crashed_session(4242),
        Err(RecoveryError::Contended)
    ));
    assert!(store.dir().join("session-4242.marker").exists());
    drop(guard);
    assert!(
        store.begin_session().is_ok(),
        "a dropped guard frees the session slot"
    );
}

#[test]
fn malformed_markers_are_ignored() {
    let (_tmp, store) = store(10);
    fs::create_dir_all(store.dir()).unwrap();
    for bad in [
        "session-0.marker",
        "session-05.marker",
        "session-x.marker",
        "session-99999999999.marker",
        "session-5.marker.bak",
    ] {
        fs::write(store.dir().join(bad), b"x").unwrap();
    }
    fs::write(store.dir().join("session-5.marker"), b"").unwrap();
    assert_eq!(store.crashed_sessions().unwrap(), vec![5]);
}

#[test]
fn a_live_sessions_retention_never_prunes_a_crashed_sessions_files() {
    let (tmp, crashed) = store(10);
    let old = crashed.write("a", b"crashed work").unwrap().entry;
    let live = RecoveryStore::new(crashed.dir(), 1, 5555).unwrap();
    for n in 1..=3u8 {
        live.write("a", &[n]).unwrap();
    }
    assert_eq!(crashed.read(&old).unwrap(), b"crashed work");
    let mine: Vec<_> = live
        .list()
        .unwrap()
        .into_iter()
        .filter(|e| e.pid == 5555)
        .collect();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].sequence, 3);
    crashed.remove(&old).unwrap();
    assert!(crashed.read(&old).is_err());
    drop(tmp);
}

#[cfg(unix)]
#[test]
fn symlink_at_the_marker_path_is_refused_and_its_target_untouched() {
    let (tmp, store) = store(10);
    fs::create_dir_all(store.dir()).unwrap();
    let target = tmp.path().join("user.txt");
    fs::write(&target, b"user data").unwrap();
    std::os::unix::fs::symlink(&target, store.dir().join("session-4242.marker")).unwrap();
    assert!(matches!(
        store.begin_session(),
        Err(RecoveryError::NotInStore)
    ));
    assert!(store.clear_crashed_session(4242).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"user data");
}

#[test]
fn invalid_pid_has_its_own_error() {
    assert!(matches!(
        RecoveryStore::new("x", 1, 0),
        Err(RecoveryError::InvalidPid)
    ));
}

#[test]
fn takeover_report_is_cleared_after_the_guard_ends() {
    let (_tmp, store) = store(10);
    fs::create_dir_all(store.dir()).unwrap();
    fs::write(store.dir().join("session-4242.marker"), b"").unwrap();
    let guard = store.begin_session().unwrap();
    guard.end().unwrap();
    assert_eq!(store.crashed_sessions().unwrap(), vec![4242]);
    store.clear_crashed_session(4242).unwrap();
    assert!(store.crashed_sessions().unwrap().is_empty());
}

#[test]
fn debug_output_omits_directory_stems_and_marker_path() {
    let (_tmp, store) = store(10);
    store.write("secret-stem", b"x").unwrap();
    let guard = store.begin_session().unwrap();
    let shown = format!("{store:?} {guard:?}");
    assert!(
        !shown.contains("secret-stem") && !shown.contains("recovery"),
        "{shown}"
    );
}
