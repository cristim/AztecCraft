//! Keyboard walk: Tab through the default scene and check what takes focus, then check that Escape
//! closes each window and the unsaved-scene prompt. Unreachable controls are recorded in
//! `gaps/keyboard.rs`; there is no other allow-list.

use super::rig::Rig;
use super::{Entry, Rule, entries};
use eframe::egui::{self, accesskit::NodeId};
use std::collections::{BTreeMap, HashSet};

const SCENARIO: &str = "keyboard";
/// Upper bound on Tab presses; the default scene has fewer stops than this.
const MAX_TABS: usize = 120;

/// What a Tab walk over `rig` found: violations keyed like the ratchet's, with their messages.
pub(super) fn walk(rig: &mut Rig) -> BTreeMap<String, String> {
    let first = entries(&rig.update(), &[]);
    let targets: Vec<&Entry> = first
        .iter()
        .filter(|e| e.actionable && !e.disabled && !e.exempt)
        .collect();
    let mut reached: HashSet<NodeId> = HashSet::new();
    let mut found = BTreeMap::new();
    let mut record = |entry: &Entry, rule: Rule| {
        let violation = entry.violation(SCENARIO, rule);
        found.insert(violation.key(), violation.message());
    };
    for _ in 0..MAX_TABS {
        rig.key(egui::Key::Tab);
        let update = rig.update();
        let focus = update.focus;
        if update.tree.as_ref().is_some_and(|tree| tree.root == focus) {
            continue;
        }
        let now = entries(&update, &[]);
        let Some(entry) = now.iter().find(|e| e.id == focus) else {
            continue;
        };
        if entry.exempt || !reached.insert(focus) {
            continue;
        }
        if entry.disabled {
            record(entry, Rule::FocusDisabled);
        }
        if entry.name.is_none() {
            record(entry, Rule::FocusUnnamed);
        }
    }
    for entry in targets {
        if !reached.contains(&entry.id) {
            record(entry, Rule::Unreachable);
        }
    }
    found
}

pub(super) fn recorded_failures() -> Vec<String> {
    let found = walk(&mut Rig::new(false));
    let known: HashSet<&str> = super::gaps::KEYBOARD.iter().copied().collect();
    let mut failures = Vec::new();
    for (key, message) in &found {
        if !known.contains(key.as_str()) {
            failures.push(format!("new in gaps/keyboard.rs: {key:?}\n    {message}"));
        }
    }
    for key in &known {
        if !found.contains_key(*key) {
            failures.push(format!("fixed, delete from gaps/keyboard.rs: {key:?}"));
        }
    }
    if !super::gaps::KEYBOARD
        .windows(2)
        .all(|pair| pair[0] < pair[1])
    {
        failures.push("gaps/keyboard.rs must be sorted without duplicates".to_owned());
    }
    failures
}

#[test]
fn tab_walk_reaches_every_enabled_control_and_stops_only_on_named_ones() {
    let failures = recorded_failures();
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

/// Whether a window titled `title` is in the tree.
fn open(rig: &Rig, title: &str) -> bool {
    rig.update()
        .nodes
        .iter()
        .any(|(_, n)| n.role() == eframe::egui::accesskit::Role::Window && n.label() == Some(title))
}

#[test]
fn escape_closes_each_window_and_the_unsaved_prompt() {
    let mut rig = Rig::new(false);
    rig.editor
        .open_primitive_options(azteccraft::scene::Primitive::Cube);
    rig.settle();
    assert!(open(&rig, "Polygon Cube Options"));
    rig.key(egui::Key::Escape);
    rig.settle();
    assert!(!open(&rig, "Polygon Cube Options"), "primitive options");

    let mut rig = Rig::new(false);
    rig.editor.open_joint_options();
    rig.settle();
    assert!(open(&rig, "Create Joints"));
    rig.key(egui::Key::Escape);
    rig.settle();
    assert!(!open(&rig, "Create Joints"), "create joints");

    let mut rig = super::scenarios::panels::face_rig();
    rig.editor.open_extrude_options();
    rig.settle();
    assert!(open(&rig, "Extrude Options"), "extrude options opened");
    rig.key(egui::Key::Escape);
    rig.settle();
    assert!(!open(&rig, "Extrude Options"), "extrude options");

    let mut rig = Rig::new(true);
    rig.editor.new_document().unwrap();
    rig.settle();
    assert!(rig.editor.files.is_modal());
    rig.key(egui::Key::Escape);
    rig.settle();
    assert!(!rig.editor.files.is_modal(), "unsaved prompt");
}
