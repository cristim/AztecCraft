//! The scenarios the gate runs, and the ratchet that compares their violations with `gaps/`.
//! A scenario other than the default only reports nodes that its base frame does not contain.
//! Add a scenario by adding a line to `all`.

mod dialogs;
pub(super) mod panels;

use super::{Violation, entries, gaps, rig::Rig, unnamed_windows, violations};
use crate::editor_operations::{ActiveTool, MenuSet, Operation};
use eframe::egui::{
    self,
    accesskit::{NodeId, Role, TreeUpdate},
};
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub(super) struct Captured {
    /// Frame before the scenario's action; its nodes are not the scenario's to report.
    pub(super) base: Option<TreeUpdate>,
    pub(super) update: TreeUpdate,
    /// Area ids of the windows the scenario opens (their chrome is exempt, see `chrome_ids`).
    pub(super) windows: Vec<egui::Id>,
    /// Title of the window the scenario must have opened.
    pub(super) window_title: Option<&'static str>,
    /// Label of a node (the expected menu row) that must be new over the base frame.
    pub(super) new_row: Option<&'static str>,
    /// Report only nodes inside a dialog and check what the dialog disables.
    pub(super) modal: bool,
}

/// What a scenario found: violations not in its base frame, and reasons it proves nothing.
pub(super) struct Run {
    pub(super) violations: Vec<Violation>,
    pub(super) problems: Vec<String>,
}

pub(super) struct Scenario {
    pub(super) name: String,
    pub(super) area: &'static str,
    pub(super) capture: Box<dyn Fn() -> Captured>,
}

impl Scenario {
    fn new(name: &str, area: &'static str, capture: impl Fn() -> Captured + 'static) -> Self {
        Scenario {
            name: name.to_owned(),
            area,
            capture: Box::new(capture),
        }
    }

    pub(super) fn run(&self) -> Run {
        let captured = (self.capture)();
        let name = &self.name;
        let mut all = violations(name, &captured.update, &captured.windows);
        if captured.modal {
            all.retain(|v| v.in_dialog);
        }
        let base_keys: HashSet<String> = captured
            .base
            .iter()
            .flat_map(|base| violations(name, base, &captured.windows))
            .map(|v| v.key())
            .collect();
        let base_ids: HashSet<NodeId> = captured
            .base
            .iter()
            .flat_map(|base| base.nodes.iter().map(|(id, _)| *id))
            .collect();
        let mut problems = Vec::new();
        if captured.modal {
            all.extend(modal_background(name, &captured));
            let dialog_opened = captured
                .update
                .nodes
                .iter()
                .any(|(id, n)| n.role() == Role::Dialog && !base_ids.contains(id));
            if !dialog_opened || newly_disabled_controls(&captured).next().is_none() {
                problems.push("no new dialog, or it disabled no control outside itself".to_owned());
            }
        }
        problems.extend(unnamed_windows(&captured.update, &captured.windows));
        let mut keys = HashSet::new();
        for violation in &all {
            if !keys.insert(violation.key()) {
                problems.push(format!("duplicate key {:?}", violation.key()));
            }
        }
        if let Some(title) = captured.window_title
            && !captured
                .update
                .nodes
                .iter()
                .any(|(_, n)| n.role() == Role::Window && n.label() == Some(title))
        {
            problems.push(format!("the window {title:?} did not open"));
        }
        let new_labels: Vec<&str> = captured
            .update
            .nodes
            .iter()
            .filter(|(id, _)| !base_ids.contains(id))
            .filter_map(|(_, n)| n.label().or(n.value()))
            .collect();
        if let Some(row) = captured.new_row
            && !new_labels.contains(&row)
        {
            problems.push(format!(
                "no new node named {row:?}; new names: {:?}",
                &new_labels[..new_labels.len().min(8)]
            ));
        }
        let added = captured
            .update
            .nodes
            .iter()
            .filter(|(id, _)| !base_ids.contains(id))
            .count();
        if captured.base.is_some() && added < MIN_NEW_NODES {
            problems.push(format!(
                "only {added} new nodes over its base frame; it exercises nothing"
            ));
        }
        Run {
            violations: all
                .into_iter()
                .filter(|v| !base_keys.contains(&v.key()))
                .collect(),
            problems,
        }
    }
}

/// A violation (at most one per scenario) when a control the dialog newly disabled does not carry
/// the reason `Unavailable: a dialog is open`.
fn modal_background(name: &str, captured: &Captured) -> Option<Violation> {
    let reason = format!("{}a dialog is open", super::DISABLED_PREFIX);
    let silent = newly_disabled_controls(captured)
        .filter(|e| e.description != reason)
        .count();
    (silent > 0).then(|| Violation {
        scenario: name.to_owned(),
        rule: super::Rule::ModalBackground,
        role: Role::Dialog,
        name: format!("{silent} controls"),
        bounds: None,
        ancestor: "root".to_owned(),
        anchor: String::new(),
        in_window: false,
        in_dialog: false,
    })
}

/// Actionable controls outside the dialog that the dialog disabled (enabled in the base frame).
fn newly_disabled_controls(captured: &Captured) -> impl Iterator<Item = super::Entry> {
    let was_disabled: HashSet<NodeId> = captured
        .base
        .iter()
        .flat_map(|base| base.nodes.iter())
        .filter(|(_, n)| n.is_disabled())
        .map(|(id, _)| *id)
        .collect();
    let base_ids: HashSet<NodeId> = captured
        .base
        .iter()
        .flat_map(|base| base.nodes.iter().map(|(id, _)| *id))
        .collect();
    entries(&captured.update, &captured.windows)
        .into_iter()
        .filter(move |e| {
            e.actionable
                && e.disabled
                && !e.in_dialog
                && base_ids.contains(&e.id)
                && !was_disabled.contains(&e.id)
        })
}

/// A scenario with a base frame must add at least this many nodes to it (the Help menu adds two
/// labels and its popup).
const MIN_NEW_NODES: usize = 2;
/// Scenarios in `all`; adding one means raising this, so a dropped line is noticed.
const SCENARIOS: usize = 32;

fn tiny_png(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("plate.png");
    let mut encoder = png::Encoder::new(std::fs::File::create(&path).unwrap(), 2, 2);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&[0; 16]).unwrap();
    writer.finish().unwrap();
    path
}

fn plain(rig: &Rig, base: Option<TreeUpdate>) -> Captured {
    Captured {
        base,
        update: rig.update(),
        windows: Vec::new(),
        window_title: None,
        new_row: None,
        modal: false,
    }
}

pub(super) fn default_frame() -> TreeUpdate {
    Rig::new(false).update()
}

fn tool_scenario(name: &str, tool: ActiveTool) -> Scenario {
    Scenario::new(name, "shell", move || {
        let mut rig = Rig::new(true);
        rig.tool(tool);
        plain(&rig, Some(default_frame()))
    })
}

/// Opens `path` (menu button, then submenus by hover) on a cube scene and reports the new nodes.
fn menu_scenario(
    name: &str,
    set: MenuSet,
    path: &'static [&'static str],
    row: &'static str,
) -> Scenario {
    Scenario::new(name, "menus", move || {
        let mut rig = Rig::new(true);
        if set != MenuSet::Modeling {
            rig.editor.native_operation(Operation::SetMenuSet { set });
            rig.settle();
        }
        let base = rig.update();
        rig.click(path[0]);
        for label in &path[1..] {
            rig.hover(label);
        }
        rig.settle();
        Captured {
            new_row: Some(row),
            ..plain(&rig, Some(base))
        }
    })
}

pub(super) fn all() -> Vec<Scenario> {
    let mut all = vec![
        Scenario::new("default", "shell", || plain(&Rig::new(false), None)),
        tool_scenario("tool-move", ActiveTool::Move),
        tool_scenario("tool-rotate", ActiveTool::Rotate),
        tool_scenario("tool-scale", ActiveTool::Scale),
    ];
    let modeling = MenuSet::Modeling;
    for (name, set, path, row) in [
        ("menu-file", modeling, &["File"][..], "Close"),
        ("menu-edit", modeling, &["Edit"], "Delete"),
        (
            "menu-create-polygon",
            modeling,
            &["Create", "Polygon Primitives"],
            "Polygon Primitives",
        ),
        (
            "menu-select-components",
            modeling,
            &["Select", "Components"],
            "Components",
        ),
        ("menu-view", modeling, &["View"], "Frame All (A)"),
        (
            "menu-view-image-plane",
            modeling,
            &["View", "Image Plane"],
            "Import Image",
        ),
        (
            "menu-shading",
            modeling,
            &["Shading"],
            "Smooth Shade All (5)",
        ),
        ("menu-modify", modeling, &["Modify"], "Not implemented yet"),
        ("menu-display", modeling, &["Display"], "Grid Options"),
        ("menu-uv", modeling, &["UV"], "UV Editor"),
        (
            "menu-skeleton",
            MenuSet::Rigging,
            &["Skeleton"],
            "Remove Joints",
        ),
        ("menu-help", modeling, &["Help"], "Local editor prototype"),
        (
            "menu-lighting-shading",
            MenuSet::Rendering,
            &["Lighting/Shading"],
            "Assign Favorite Material",
        ),
        (
            "menu-assign-new-material",
            MenuSet::Rendering,
            &["Lighting/Shading", "Assign New Material"],
            "Lambert",
        ),
        (
            "menu-assign-existing-material",
            MenuSet::Rendering,
            &["Lighting/Shading", "Assign Existing Material"],
            "standardSurface1",
        ),
    ] {
        all.push(menu_scenario(name, set, path, row));
    }
    // With a plane the Remove row is enabled and must not carry a stale reason (R3b).
    all.push(Scenario::new("menu-view-image-plane-set", "menus", || {
        let dir = tempfile::tempdir().unwrap();
        let path = tiny_png(dir.path());
        let mut rig = Rig::new(true);
        rig.editor.run_import_image(path);
        rig.settle();
        let base = rig.update();
        rig.click("View");
        rig.hover("Image Plane");
        rig.settle();
        Captured {
            new_row: Some("Remove Image Plane"),
            ..plain(&rig, Some(base))
        }
    }));
    all.extend(panels::scenarios());
    all.extend(dialogs::scenarios());
    all
}

/// Failure lines: new violations (fix the control, or add its key to the area file) and recorded
/// keys no longer violated (delete the line).
pub(super) fn ratchet_failures(
    scenarios: &[Scenario],
    recorded: &[(&str, &[&str])],
) -> Vec<String> {
    let mut found: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    let mut failures_problems = Vec::new();
    for scenario in scenarios {
        let area = found.entry(scenario.area).or_default();
        let run = scenario.run();
        for violation in run.violations {
            area.insert(violation.key(), violation.message());
        }
        failures_problems.extend(
            run.problems
                .into_iter()
                .map(|problem| format!("[{}] {problem}", scenario.name)),
        );
    }
    let mut failures = failures_problems;
    for (area, keys) in &found {
        let known: BTreeSet<&str> = recorded
            .iter()
            .filter(|(name, _)| name == area)
            .flat_map(|(_, keys)| keys.iter().copied())
            .collect();
        for (key, message) in keys {
            if !known.contains(key.as_str()) {
                failures.push(format!("new in gaps/{area}.rs: {key:?}\n    {message}"));
            }
        }
        for key in &known {
            if !keys.contains_key(*key) {
                failures.push(format!("fixed, delete from gaps/{area}.rs: {key:?}"));
            }
        }
    }
    for (area, keys) in recorded {
        if !found.contains_key(area) {
            failures.push(format!("gaps/{area}.rs has no scenario"));
        }
        if !keys.windows(2).all(|pair| pair[0] < pair[1]) {
            failures.push(format!("gaps/{area}.rs must be sorted without duplicates"));
        }
    }
    failures
}

#[test]
fn gate_matches_the_recorded_gaps() {
    let scenarios = all();
    assert_eq!(
        scenarios.len(),
        SCENARIOS,
        "update SCENARIOS with the line added to or dropped from `all`"
    );
    let failures = ratchet_failures(&scenarios, &gaps::recorded());
    assert!(failures.is_empty(), "\n{}\n", failures.join("\n"));
}

/// (disabled, description) of the node named `label` in the open menu.
fn row(rig: &Rig, label: &str) -> (bool, String) {
    let update = rig.update();
    let (_, node) = update
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some(label))
        .unwrap_or_else(|| panic!("no row named {label:?}"));
    (
        node.is_disabled(),
        node.description().unwrap_or("").to_owned(),
    )
}

#[test]
fn workspace_selector_is_disabled_at_the_right_of_the_main_menu() {
    let mut rig = Rig::new(true);
    let scene = rig.editor.history.scene().clone();
    let token = rig.editor.scene_token;
    for set in [MenuSet::Modeling, MenuSet::Rigging, MenuSet::Rendering] {
        rig.editor.native_operation(Operation::SetMenuSet { set });
        rig.settle();
        assert_eq!(
            row(&rig, "Workspaces unavailable"),
            (
                true,
                "Unavailable: Workspace switching and docking are not implemented yet".into()
            )
        );
        let selector = rig.find("Workspaces unavailable");
        let file = rig.find("File");
        assert!((selector.y - file.y).abs() < 2.0);
        assert!(selector.x > super::rig::SIZE.x - 120.0);
        rig.click("Workspaces unavailable");
        rig.settle();
        assert_eq!(rig.editor.menu_set, set);
        assert_eq!(rig.editor.scene_token, token);
        assert_eq!(rig.editor.history.scene(), &scene);
    }
}

#[test]
fn the_image_plane_menu_rows_follow_the_plane_state_with_their_reasons() {
    let open = |rig: &mut Rig| {
        rig.click("View");
        rig.hover("Image Plane");
        rig.settle();
    };
    let mut none = Rig::new(true);
    open(&mut none);
    assert_eq!(row(&none, "Import Image"), (false, String::new()));
    let (disabled, reason) = row(&none, "Remove Image Plane");
    assert!(
        disabled && reason == "Unavailable: There is no image plane to remove",
        "{reason}"
    );
    let (disabled, reason) = row(&none, "Image Plane Attributes");
    assert!(
        disabled && reason == "Unavailable: There is no image plane: use Import Image first",
        "{reason}"
    );

    let dir = tempfile::tempdir().unwrap();
    let mut some = Rig::new(true);
    some.editor.run_import_image(tiny_png(dir.path()));
    some.settle();
    open(&mut some);
    assert_eq!(row(&some, "Remove Image Plane"), (false, String::new()));
    assert_eq!(row(&some, "Image Plane Attributes"), (false, String::new()));
    // Both rows follow the stored plane, not what is drawn: with an orthographic camera nothing
    // is drawn, and they stay enabled.
    some.editor.native_operation(Operation::SetView(
        crate::editor_operations::ViewPreset::Top,
    ));
    some.settle();
    assert!(some.editor.plate_draw().is_none());
    assert_eq!(row(&some, "Remove Image Plane"), (false, String::new()));
    assert_eq!(row(&some, "Image Plane Attributes"), (false, String::new()));
}
