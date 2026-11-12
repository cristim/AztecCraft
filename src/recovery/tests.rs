use super::*;

fn store(max: usize) -> (tempfile::TempDir, RecoveryStore) {
    let tmp = tempfile::tempdir().unwrap();
    let store = RecoveryStore::new(tmp.path().join("recovery"), max, 4242).unwrap();
    (tmp, store)
}

fn names(store: &RecoveryStore) -> Vec<String> {
    let mut found: Vec<String> = fs::read_dir(store.dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

#[test]
fn sanitize_boundaries() {
    assert_eq!(sanitize_stem("my scene.mb"), "my_scene_mb");
    assert_eq!(sanitize_stem(""), "untitled");
    assert_eq!(sanitize_stem("../../etc/passwd"), "______etc_passwd");
    assert_eq!(sanitize_stem(&"a".repeat(200)).len(), MAX_STEM_CHARS);
    assert_eq!(sanitize_stem("é"), "_");
    assert_eq!(sanitize_stem("ok-Name_1"), "ok-Name_1");
}

#[test]
fn name_round_trip_and_rejections() {
    assert_eq!(
        parse_name(&file_name("scene", 4242, 7)),
        Some(("scene", 4242, 7))
    );
    assert_eq!(
        file_name("scene", 4242, 7),
        "scene.autosave.p4242.000007.recovery"
    );
    for bad in [
        "scene.mb",
        "scene.autosave.p4242.7.recovery",
        "scene.autosave.p4242.0000007.recovery",
        "scene.autosave.p4242.00000a.recovery",
        ".autosave.p4242.000001.recovery",
        "a b.autosave.p4242.000001.recovery",
        "scene.autosave.p4242.000001.recovery.bak",
        "scene.autosave.p4242.+00001.recovery",
        "scene.autosave.000001.recovery",
        "scene.autosave.p0.000001.recovery",
        "scene.autosave.p042.000001.recovery",
        "scene.autosave.pa.000001.recovery",
    ] {
        assert_eq!(parse_name(bad), None, "{bad}");
    }
}

#[test]
fn zero_retention_is_rejected() {
    assert!(matches!(
        RecoveryStore::new("x", 0, 1),
        Err(RecoveryError::InvalidRetention)
    ));
}

#[test]
fn write_creates_dir_numbers_files_and_reads_back() {
    let (_tmp, store) = store(10);
    let first = store.write("my scene", b"one").unwrap();
    let second = store.write("my scene", b"two").unwrap();
    assert_eq!(
        first.entry.file_name,
        "my_scene.autosave.p4242.000001.recovery"
    );
    assert_eq!(second.entry.sequence, 2);
    assert_eq!(store.read(&second.entry).unwrap(), b"two");
    let listed = store.list().unwrap();
    assert_eq!(listed.len(), 2);
    assert_eq!(listed[0].len, 3);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(store.dir().join(&first.entry.file_name))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0, "recovery files are private");
    }
}

#[test]
fn empty_and_oversize_payloads_touch_nothing() {
    let (_tmp, store) = store(3);
    assert!(matches!(
        store.write("s", b""),
        Err(RecoveryError::EmptyPayload)
    ));
    let big = vec![0u8; MAX_RECOVERY_BYTES + 1];
    assert!(matches!(
        store.write("s", &big),
        Err(RecoveryError::TooLarge)
    ));
    assert!(!store.dir().exists());
}

#[test]
fn sequence_continues_after_gaps_and_stems_are_independent() {
    let (_tmp, store) = store(10);
    store.write("a", b"1").unwrap();
    store.write("a", b"2").unwrap();
    fs::remove_file(store.dir().join(file_name("a", 4242, 1))).unwrap();
    assert_eq!(store.write("a", b"3").unwrap().entry.sequence, 3);
    assert_eq!(store.write("b", b"1").unwrap().entry.sequence, 1);
}

#[test]
fn retention_prunes_oldest_of_the_same_stem_only() {
    let (_tmp, store) = store(2);
    store.write("other", b"keep").unwrap();
    for n in 0..4u8 {
        let out = store.write("a", &[n + 1]).unwrap();
        assert_eq!(out.prune_failures, 0);
    }
    let a: Vec<u32> = store
        .list()
        .unwrap()
        .iter()
        .filter(|e| e.stem == "a")
        .map(|e| e.sequence)
        .collect();
    assert_eq!(a, vec![3, 4]);
    assert_eq!(
        store
            .list()
            .unwrap()
            .iter()
            .filter(|e| e.stem == "other")
            .count(),
        1
    );
}

#[test]
fn prune_never_touches_foreign_files() {
    let (_tmp, store) = store(1);
    store.write("a", b"1").unwrap();
    for foreign in [
        "scene.mb",
        "a.autosave.p4242.1.recovery",
        "notes.txt",
        "a.autosave.p4242.000001.recovery.bak",
    ] {
        fs::write(store.dir().join(foreign), b"user data").unwrap();
    }
    store.write("a", b"2").unwrap();
    store.write("a", b"3").unwrap();
    for foreign in [
        "scene.mb",
        "a.autosave.p4242.1.recovery",
        "notes.txt",
        "a.autosave.p4242.000001.recovery.bak",
    ] {
        assert_eq!(
            fs::read(store.dir().join(foreign)).unwrap(),
            b"user data",
            "{foreign}"
        );
    }
}

#[test]
fn occupied_destinations_are_preserved_and_skipped() {
    let (_tmp, store) = store(10);
    store.write("a", b"1").unwrap();
    // A directory and a symlink squatting on the next names: never replaced, never reused.
    fs::create_dir(store.dir().join(file_name("a", 4242, 2))).unwrap();
    #[cfg(unix)]
    {
        let target = store.dir().join("victim.txt");
        fs::write(&target, b"victim").unwrap();
        std::os::unix::fs::symlink(&target, store.dir().join(file_name("a", 4242, 3))).unwrap();
        let out = store.write("a", b"new").unwrap();
        assert_eq!(out.entry.sequence, 4);
        assert_eq!(fs::read(&target).unwrap(), b"victim");
    }
    #[cfg(not(unix))]
    assert_eq!(store.write("a", b"new").unwrap().entry.sequence, 3);
    assert!(store.dir().join(file_name("a", 4242, 2)).is_dir());
    assert_eq!(
        fs::read(store.dir().join(file_name("a", 4242, 1))).unwrap(),
        b"1"
    );
}

#[test]
fn a_reused_pid_never_prunes_the_earlier_processs_files() {
    let (_tmp, first) = store(10);
    let old = first.write("a", b"crashed work").unwrap().entry;
    // A later process gets the same pid and a tight retention limit.
    let second = RecoveryStore::new(first.dir(), 1, 4242).unwrap();
    for n in 1..=3u8 {
        second.write("a", &[n]).unwrap();
    }
    assert_eq!(second.read(&old).unwrap(), b"crashed work");
    assert_eq!(second.list().unwrap().len(), 2);
}

mod session;

mod failures;

#[test]
fn retention_prunes_only_files_this_instance_wrote_even_with_the_same_pid() {
    let (_tmp, a) = store(2);
    let b = RecoveryStore::new(a.dir(), 2, 4242).unwrap();
    a.write("untitled", b"a1").unwrap();
    let theirs = b.write("untitled", b"b1").unwrap().entry;
    for n in 0..3u8 {
        a.write("untitled", &[n]).unwrap();
    }
    assert_eq!(b.read(&theirs).unwrap(), b"b1");
    let mine = a.list().unwrap().len();
    assert_eq!(mine, 3, "two of a's files plus b's");
}

#[test]
fn stems_differing_only_in_case_share_one_sequence() {
    let (_tmp, store) = store(10);
    store.write("Scene", b"1").unwrap();
    assert_eq!(store.next_sequence("scene").unwrap(), 2);
    let out = store.write("scene", b"2").unwrap();
    assert_eq!(out.entry.sequence, 2);
}

#[test]
fn remove_through_the_writing_store_frees_its_bookkeeping() {
    let (_tmp, a) = store(1);
    let first = a.write("a", b"1").unwrap().entry;
    a.remove(&first).unwrap();
    let b = RecoveryStore::new(a.dir(), 1, 4242).unwrap();
    let theirs = b.write("a", b"b").unwrap().entry;
    a.write("a", b"2").unwrap();
    assert_eq!(b.read(&theirs).unwrap(), b"b");
}
