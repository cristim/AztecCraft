//! Reproducible history memory measurement (issue #25).
//! Run: cargo test --release --test history_memory -- --ignored --nocapture
//! Baseline: copy this file onto main a0722217 (unbounded history) and run the
//! same command. Unix only: it reads RSS with `ps -o rss=`.
#![cfg(unix)]

use azteccraft::history::{Command, History};
use azteccraft::native_scene::NativeScene;
use azteccraft::scene::Primitive;

fn rss_kib() -> u64 {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
}

#[test]
#[ignore = "measurement, not a regression test"]
fn rss_growth_over_many_edits_on_a_large_scene() {
    const NODES: usize = 1000;
    const EDITS: usize = 300;
    let mut scene = NativeScene::default();
    let mut first = None;
    for n in 0..NODES {
        let id = scene.create(Primitive::Cube, &format!("cube{n}")).unwrap();
        first.get_or_insert(id);
    }
    let id = first.unwrap();
    let mut history = History::new(scene);
    let before = rss_kib();
    for n in 0..EDITS {
        history
            .execute(Command::Rename {
                id,
                name: format!("edit{n}"),
            })
            .unwrap();
    }
    let after = rss_kib();
    println!(
        "nodes={NODES} edits={EDITS} rss_before_kib={before} rss_after_kib={after} growth_kib={}",
        after.saturating_sub(before)
    );
}
